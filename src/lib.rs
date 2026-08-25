//! DuckDB extension exposing phonetic matching algorithms from the
//! [`rphonetic`](https://crates.io/crates/rphonetic) crate, a Rust port of the
//! phonetic encoders in Apache Commons Codec.
//!
//! Two functions are registered:
//!
//! * `cologne_phonetic(VARCHAR) -> VARCHAR` — Kölner Phonetik, a single code.
//! * `daitch_mokotoff(VARCHAR) -> LIST(VARCHAR)` — Daitch-Mokotoff Soundex,
//!   which yields *several* codes for ambiguous spellings. Match two names by
//!   testing the two lists for overlap with `list_has_any`.
//!
//! Both propagate NULL: a NULL input row produces a NULL output row.

use std::sync::LazyLock;

use duckdb::{
    Connection, Result,
    core::{DataChunkHandle, Inserter, LogicalTypeHandle, LogicalTypeId},
    duckdb_entrypoint_c_api,
    ffi::duckdb_string_t,
    types::DuckString,
    vscalar::{ScalarFunctionSignature, VScalar},
    vtab::arrow::WritableVector,
};
use rphonetic_lib::{Cologne, DaitchMokotoffSoundex, Encoder};

/// Parsing the Daitch-Mokotoff rule set is not free, so do it once per process.
static DAITCH_MOKOTOFF: LazyLock<DaitchMokotoffSoundex> = LazyLock::new(DaitchMokotoffSoundex::default);

/// Daitch-Mokotoff codes are consumed as a *set* (two names match when their
/// code lists overlap), so a branch that produces a code already in the list
/// carries no information. `rphonetic` can emit such repeats for inputs
/// containing separators; Commons Codec's `soundex()` does not. Dropping them
/// keeps the two in agreement. Order is preserved: the lists are short enough
/// that the quadratic scan is cheaper than allocating a hash set.
fn dedupe(mut codes: Vec<String>) -> Vec<String> {
    let mut kept = 0;
    for i in 0..codes.len() {
        if !codes[..kept].contains(&codes[i]) {
            codes.swap(kept, i);
            kept += 1;
        }
    }
    codes.truncate(kept);
    codes
}

/// Read column 0 of `input` as strings, mapping NULL rows to `None`.
fn read_varchar_column(input: &mut DataChunkHandle) -> Vec<Option<String>> {
    let rows = input.len();
    let vector = input.flat_vector(0);
    let values = unsafe { vector.as_slice_with_len::<duckdb_string_t>(rows) };

    values
        .iter()
        .take(rows)
        .enumerate()
        .map(|(row, value)| {
            if vector.row_is_null(row as u64) {
                None
            } else {
                let mut value = *value;
                Some(DuckString::new(&mut value).as_str().to_string())
            }
        })
        .collect()
}

/// `cologne_phonetic(VARCHAR) -> VARCHAR`
struct ColognePhonetic;

impl VScalar for ColognePhonetic {
    type State = ();

    fn invoke(
        _state: &Self::State,
        input: &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let inputs = read_varchar_column(input);
        let mut output = output.flat_vector();

        for (row, value) in inputs.iter().enumerate() {
            match value {
                None => output.set_null(row),
                Some(value) => output.insert(row, Cologne.encode(value).as_str()),
            }
        }
        Ok(())
    }

    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![ScalarFunctionSignature::exact(
            vec![LogicalTypeId::Varchar.into()],
            LogicalTypeId::Varchar.into(),
        )]
    }
}

/// `daitch_mokotoff(VARCHAR) -> LIST(VARCHAR)`
struct DaitchMokotoff;

impl VScalar for DaitchMokotoff {
    type State = ();

    fn invoke(
        _state: &Self::State,
        input: &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Encode the whole chunk first: the child vector must be reserved once
        // at its final size, because a later, larger reserve may reallocate the
        // child buffer and invalidate strings already written through it.
        let encoded: Vec<Option<Vec<String>>> = read_varchar_column(input)
            .iter()
            .map(|value| {
                value
                    .as_ref()
                    .map(|value| dedupe(DAITCH_MOKOTOFF.inner_soundex(value, true)))
            })
            .collect();
        let total: usize = encoded.iter().map(|codes| codes.as_ref().map_or(0, Vec::len)).sum();

        let mut list = output.list_vector();
        // `child` panics on a rejected reserve; reserving at least one element
        // keeps the call away from the zero-length edge of the C API.
        let child = list.child(total.max(1));

        let mut offset = 0usize;
        for (row, codes) in encoded.iter().enumerate() {
            match codes {
                None => {
                    // Write the entry as well as the validity bit, so DuckDB
                    // never reads an uninitialised offset/length pair.
                    list.set_entry(row, offset, 0);
                    list.set_null(row);
                }
                Some(codes) => {
                    for (i, code) in codes.iter().enumerate() {
                        child.insert(offset + i, code.as_str());
                    }
                    list.set_entry(row, offset, codes.len());
                    offset += codes.len();
                }
            }
        }
        list.set_len(total);
        Ok(())
    }

    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![ScalarFunctionSignature::exact(
            vec![LogicalTypeId::Varchar.into()],
            LogicalTypeHandle::list(&LogicalTypeId::Varchar.into()),
        )]
    }
}

#[duckdb_entrypoint_c_api]
pub unsafe fn extension_entrypoint(con: Connection) -> Result<(), Box<dyn std::error::Error>> {
    con.register_scalar_function::<ColognePhonetic>("cologne_phonetic")?;
    con.register_scalar_function::<DaitchMokotoff>("daitch_mokotoff")?;
    Ok(())
}
