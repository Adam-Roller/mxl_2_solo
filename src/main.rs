use std::fs::File;
use std::io::{BufReader, Write};

use xml::reader::{EventReader, XmlEvent};

mod partwise;

fn main() -> std::io::Result<()> {
    let dialog_result = wfd::open_dialog(Default::default()).unwrap();
    let file = File::open(dialog_result.selected_file_path).unwrap();
    let file = BufReader::new(file);
    let mut parser = EventReader::new(file);
    let mut score = partwise::Score::new();

    loop{
        match parser.next() {
            Ok(XmlEvent::StartElement {name, ..}) => {
                match name.local_name.as_str() {
                    "score-partwise" => {
                        score = partwise::Score::parse_score(&mut parser);
                    }
                    _ => {}
                }
            }
            Ok(XmlEvent::EndElement {..}) => {
            }
            Ok(XmlEvent::EndDocument) => {
                let mut outfile = File::create("output.gjm").unwrap();
                // File Version
                let line = "Version ='1.1.0.0'\n";
                outfile.write_all(line.as_bytes())?;

                // Overall Notation info
                let line = "Notation = {\n";
                outfile.write_all(line.as_bytes())?;
                //      Version and author info
                let line = "\tVersion ='1.1.0.0',\n\tNotationName = 'Unnamed',\n\tNotationAuther = 'UnknownAuthor',\n\tNotationTranslater = 'UnknownTranslator',\n\tNotationCreator = 'Dwarfed',\n\tVolume = 1,\n";
                outfile.write_all(line.as_bytes())?;
                //      Time signature info
                let line = format!("\tBeatsPerMeasure = {},\n", score.get_beats_per_measure());
                outfile.write_all(line.as_bytes())?;
                let line = format!("\tBeatDurationType = '{}',\n", score.get_beat_duration_type());
                outfile.write_all(line.as_bytes())?;
                let line = "\tNumberedKeySignature = 'C',\n";
                outfile.write_all(line.as_bytes())?;

                //      BPM
                let line = "\tMeasureBeatsPerMinuteMap = {\n";
                outfile.write_all(line.as_bytes())?;
                let line = score.get_bpm_map();
                outfile.write_all(line.as_bytes())?;
                let line = "\t},\n";
                outfile.write_all(line.as_bytes())?;

                //      Number of Measures
                let line = format!("\tMeasureAlignedCount = {},\n", score.get_measure_count());
                outfile.write_all(line.as_bytes())?;

                // Close notation info
                let line = "}\n";
                outfile.write_all(line.as_bytes())?;

                // Track/measure/note info
                score.write_score_gjn(&mut outfile)?;
                break;
            }
            Err(e) => {
                println!("Error: {}", e);
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
