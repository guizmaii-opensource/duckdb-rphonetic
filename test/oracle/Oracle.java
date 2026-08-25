// Reference oracle: runs Apache Commons Codec's own ColognePhonetic and
// DaitchMokotoffSoundex over test/corpus/names.txt and writes the results to
// test/corpus/commons-codec.tsv.
//
// The extension is built on the `rphonetic` crate, which is a Rust port of
// these two encoders, so its output should agree with them. `test/oracle/run.sh`
// regenerates the TSV; test/sql/commons_codec_corpus.test asserts the extension
// matches it, with the known Cologne divergences listed in
// test/corpus/cologne-divergences.tsv.
//
// Usage (Java 11+, single-file source launch):
//   java -cp commons-codec-1.22.1.jar test/oracle/Oracle.java <names.txt> <out.tsv>

import java.io.IOException;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.apache.commons.codec.language.ColognePhonetic;
import org.apache.commons.codec.language.DaitchMokotoffSoundex;

public class Oracle {
    public static void main(String[] args) throws IOException {
        Path in = Path.of(args[0]);
        Path out = Path.of(args[1]);

        ColognePhonetic cologne = new ColognePhonetic();
        DaitchMokotoffSoundex daitchMokotoff = new DaitchMokotoffSoundex();
        List<String> names = Files.readAllLines(in, StandardCharsets.UTF_8);

        try (PrintWriter w = new PrintWriter(Files.newBufferedWriter(out, StandardCharsets.UTF_8))) {
            w.println("name\tcologne\tdaitch_mokotoff");
            for (String name : names) {
                if (name.isEmpty()) {
                    continue;
                }
                w.println(name + "\t" + cologne.encode(name) + "\t" + daitchMokotoff.soundex(name));
            }
        }
    }
}
