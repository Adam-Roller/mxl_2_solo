use std::fs::File;
use std::fmt::Write;
use std::io::{BufReader, Write as OtherWrite};
use std::collections::BTreeMap;
use xml::reader::{EventReader, XmlEvent};

const MAX_PART_COUNT: usize = 3;

fn indent(cnt: usize) -> String {
    let mut ind = "".to_string();
    for _ in 0..cnt {
        ind = format!("{}{}", ind, "\t");
    }
    ind
}

fn calc_measure_maps(measures: &Vec<Measure>) -> (Vec<(usize, i32)>, Vec<(usize, Clef)>, Vec<(usize, u32)>) {
    let mut key_sigs = Vec::<(usize, i32)>::new();
    let mut clefs = Vec::<(usize, Clef)>::new();
    let mut volumes = Vec::<(usize, u32)>::new();

    if let Some(measure) = measures.first() {
        let mut last_key_sig = measure.attributes.key;
        key_sigs.push((0, last_key_sig));

        let mut last_clef = measure.attributes.clef;
        clefs.push((0, last_clef));

        let mut last_volume = measure.attributes.volume;
        volumes.push((0, last_volume));

        for (i, measure) in measures.iter().enumerate() {
            if measure.attributes.key != last_key_sig {
                last_key_sig = measure.attributes.key;
                key_sigs.push((i, last_key_sig));
            }
            if measure.attributes.clef != last_clef {
                last_clef = measure.attributes.clef;
                clefs.push((i, last_clef));
            }
            if measure.attributes.volume != last_volume {
                last_volume = measure.attributes.volume;
                volumes.push((i, last_volume));
            }
        }
    }

    (key_sigs, clefs, volumes)
}

/// Parses the internal value of a tag. This function expects that the provided parser is already
/// inside the tag specified by label, that the tag only has characters inside of it, 
/// and will only return once it has parsed the closing tag with that same label.
///
/// # Arguments
/// 
/// * 'label' - A string slice holding the label of the tag to parse
/// * 'parser' - A mutable reference to the parser located inside the referenced tag
///
fn parse_tag_value(label: &str, parser: &mut EventReader<BufReader<File>>) -> String {
    let mut value: String = "".to_string();
    match parser.next(){
        Ok(XmlEvent::Characters(chars)) => {
            value = chars;
        }
        _ => {println!("Warning! Non-Characters Element inside <{}>", label);}
    }
    loop {
        match parser.next(){
            Ok(XmlEvent::EndElement{name}) => {
                if name.local_name.as_str() == label {
                    break;
                }
            }
            _ => {println!("Warning! Extra Elements inside <{}>", label);}
        }
    }
    value
}

/// An enum to hold the duration value of a single note
#[derive(Clone, Copy, Debug)]
enum NoteType {
    TenTwentyFourth,
    FiveTwelfth,
    TwoFiftySixth,
    OneTwentyEighth,
    SixtyFourth,
    // Nothing shorter than ThirtySecond is supported by GJM
    ThirtySecond,
    Sixteenth,
    Eighth,
    Quarter,
    Half,
    Whole,
    // Nothing longer than Whole is supported by GJM
    Breve,
    Long,
    Maxima,
}

/// A Representation of a single note
#[derive(Clone, Debug)]
struct Note {
    /// The numeric note value with index zero being A1 and increasing by one each half step
    pitch_index: u32,
    /// Note alteration in half steps, i.e. a flat note has alter = -1
    alter: i32,
    /// Duration of the note in divisions
    duration: u32,
    /// Note duration type as an enum
    note_type: NoteType,
    /// In multi-staff parts staff is used to track which staff each note sits on
    staff: u8,
    /// Whether the note is a rest or not
    is_rest: bool,
    /// Whether the note is dotted
    dotted: bool,
    /// Whether the note is arpeggiated
    arpeggiate: bool,
    /// Whether the note is the start of a triplet
    triplet: bool,
    /// Whether a slur/tie starts on this note
    slur_start: bool,
    /// Whether a slur/tie stops on this note
    slur_stop: bool,
}

impl Note {
    /// Returns a default instantiation of a Note
    fn new() -> Self {
        Note {
            pitch_index: 0,
            alter: 0,
            duration: 0,
            note_type: NoteType::Quarter,
            staff: 1,
            is_rest: false,
            dotted: false,
            arpeggiate: false,
            triplet: false,
            slur_start: false,
            slur_stop: false,
        }
    }

    /// Converts from MusicXml "step" and "octave" into a pitch index
    fn convert_pitch_index(step: &str, octave: u32) -> u32 {
        // Each octave has 12 pitch indexes and octave starts at one, not zero.
        let mut pitch_index;
        if octave == 0 {
            pitch_index = octave * 12;
        } else {
            pitch_index = (octave - 1) * 12;
        }
        // The note index is how many half steps from A flat the note is.
        match step {
            "A" => {
                pitch_index += 13;
            }
            "B" => {
                pitch_index += 15;
            }
            "C" => {
                pitch_index += 4;
            }
            "D" => {
                pitch_index += 6;
            }
            "E" => {
                pitch_index += 8;
            }
            "F" => {
                pitch_index += 9;
            }
            "G" => {
                pitch_index += 11;
            }
            _ => {}
        }
        pitch_index
    }

    /// Parses the tags and values within a "note" tag, returning the constructed Note and whether
    /// it is part of a previously started chord
    ///
    /// # Arguments
    ///
    /// * 'parser' - A mutable reference to the parser located inside the "note" tag
    ///
    /// Returns a Tuple of the (Note, is_a_chord)
    ///
    fn parse_note(parser: &mut EventReader<BufReader<File>>) -> (Self, bool) {
        let mut note = Note::new();
        let mut is_chord = false;
        loop {
            match parser.next() {
                Ok(XmlEvent::StartElement {name, ..}) => {
                    match name.local_name.as_str() {
                        "pitch" => {
                            let mut step = "".to_string();
                            let mut octave: u32 = 0;
                            loop {
                                match parser.next() {
                                    Ok(XmlEvent::StartElement {name, ..}) => {
                                        match name.local_name.as_str() {
                                            "step" => {
                                                step = parse_tag_value("step", parser);
                                            }
                                            "octave" => {
                                                octave = parse_tag_value("octave", parser).parse::<u32>().unwrap();
                                            }
                                            "alter" => {
                                                note.alter = parse_tag_value("alter", parser).parse::<i32>().unwrap();
                                            }
                                            _ => {}
                                        }
                                    }
                                    Ok(XmlEvent::EndElement {name}) => {
                                        if name.local_name.as_str() == "pitch" {
                                            note.pitch_index = Note::convert_pitch_index(step.as_str(), octave);
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "chord" => {
                            is_chord = true;
                        }
                        "type" => {
                            match parse_tag_value("type", parser).as_str() {
                                "1024th" => {
                                    note.note_type = NoteType::TenTwentyFourth;
                                }
                                "512th" => {
                                    note.note_type = NoteType::FiveTwelfth;
                                }
                                "256th" => {
                                    note.note_type = NoteType::TwoFiftySixth;
                                }
                                "128th" => {
                                    note.note_type = NoteType::OneTwentyEighth;
                                }
                                "64th" => {
                                    note.note_type = NoteType::SixtyFourth;
                                }
                                "32nd" => {
                                    note.note_type = NoteType::ThirtySecond;
                                }
                                "16th" => {
                                    note.note_type = NoteType::Sixteenth;
                                }
                                "eighth" => {
                                    note.note_type = NoteType::Eighth;
                                }
                                "quarter" => {
                                    note.note_type = NoteType::Quarter;
                                }
                                "half" => {
                                    note.note_type = NoteType::Half;
                                }
                                "whole" => {
                                    note.note_type = NoteType::Whole;
                                }
                                "breve" => {
                                    note.note_type = NoteType::Breve;
                                }
                                "long" => {
                                    note.note_type = NoteType::Long;
                                }
                                "maxima" => {
                                    note.note_type = NoteType::Maxima;
                                }
                                _ => {}
                            }
                        }
                        "duration" => {
                            note.duration = parse_tag_value("duration", parser).parse::<u32>().unwrap();
                        }
                        "staff" => {
                            note.staff = parse_tag_value("staff", parser).parse::<u8>().unwrap();
                        }
                        "rest" => {
                            note.is_rest = true;
                        }
                        "dot" => {
                            note.dotted = true;
                        }
                        "notations" => {
                            loop {
                                match parser.next() {
                                    Ok(XmlEvent::StartElement {name, attributes, ..}) => {
                                        match name.local_name.as_str() {
                                            "arpeggiate" => {
                                                note.arpeggiate = true;
                                            }
                                            "tuplet" => {
                                                if !attributes.is_empty() {
                                                    for attr in attributes {
                                                        if attr.name.local_name.as_str() == "type" {
                                                            if attr.value == "start" {
                                                                note.triplet = true;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            "slur" => {
                                                if !attributes.is_empty() {
                                                    for attr in attributes {
                                                        if attr.name.local_name.as_str() == "type" {
                                                            if attr.value == "start" {
                                                                note.slur_start = true;
                                                            } else if attr.value == "stop" {
                                                                note.slur_stop = true;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            "tied" => {
                                                if !attributes.is_empty() {
                                                    for attr in attributes {
                                                        if attr.name.local_name.as_str() == "type" {
                                                            if attr.value == "start" {
                                                                note.slur_start = true;
                                                            } else if attr.value == "stop" {
                                                                note.slur_stop = true;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    Ok(XmlEvent::EndElement {name}) => {
                                        if name.local_name.as_str() == "notations" {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(XmlEvent::EndElement {name}) => {
                    if name.local_name.as_str() == "note" {
                        break;
                    }
                }
                _ => {}
            }
        }

        (note, is_chord)
    }

    fn get_numbered_sign(&self) -> u32 {
        // Each octave has 12 pitch indexes and octave starts at one, not zero.
        let index = self.pitch_index % 12;
        let mut value = 1;
        // The note index is how many half steps from A flat the note is.
        match index {
            1 => {
                value = 1;
            }
            3 => {
                value = 2;
            }
            4 => {
                value = 3;
            }
            6 => {
                value = 4;
            }
            8 => {
                value = 5;
            }
            9 => {
                value = 6;
            }
            11 => {
                value = 7;
            }
            _ => {}
        }
        value
    }

    fn get_alterant_type(&self) -> &str {
        let mut result = "";
        match self.alter {
            -1 => {result = "Flat";},
            0 => {result = "Natural";},
            1 => {result = "Sharp";},
            _ => {},
        }
        result
    }
}

/// A collection of Notes that all begin on the same division
#[derive(Clone, Debug)]
struct Chord {
    /// The Notes of the Chord
    notes: Vec<Note>,
    /// The division the chord begins on
    start_time: u32,
    duration: u32,
    note_type: NoteType,
    dotted: bool,
    is_rest: bool,
    arpeggiate: bool,
    triplet: bool,
    slur_start: bool,
    slur_stop: bool,
}

impl Chord {
    /// Returns a default instantiation of a Chord
    fn new() -> Self {
        Self {
            notes: Vec::<Note>::new(),
            start_time: 0,
            duration: 0,
            note_type: NoteType::Quarter,
            dotted: false,
            is_rest: false,
            arpeggiate: false,
            triplet: false,
            slur_start: false,
            slur_stop: false,
        }
    }

    fn gjm_note_string(&self) -> &str{
        let mut value = "";
        match self.note_type {
            NoteType::ThirtySecond => {
                value = "The32nd";
            },
            NoteType::Sixteenth => {
                value = "The16th";
            },
            NoteType::Eighth => {
                value = "Eighth";
            },
            NoteType::Quarter => {
                value = "Quarter";
            },
            NoteType::Half => {
                value = "Half";
            },
            NoteType::Whole => {
                value = "Whole";
            },
            _ => {}
        }
        value
    }

    fn gjm_duration(&self, ratio: f64) -> u32 {
        (self.duration as f64 * ratio).round() as u32
    }
}

/// Enumerated Clef sign values
#[derive(Clone, Debug, Copy, PartialEq)]
enum Clef {
    F,  // Treble Clef
    G,  // Bass Clef
}

/// A collection of attributes that apply to measures
#[derive(Clone, Debug)]
struct Attributes {
    /// Number of divisions per beat
    divisions: u32,
    /// Volume out of 100
    volume: u32,
    /// Beats per minute
    tempo: u32,
    /// The major key represented by a shift from C Major, i.e. Bflat Major would have key = -2
    key: i32,
    /// The number of beats per measure (the top of the key signature)
    beats: u8,
    /// What type of note counts as a beat (the bottom of the key signature)
    beat_type: u8,
    /// What Clef the associated measure uses
    clef: Clef,
}

impl Attributes {
    /// Returns a deafult instantiation of the Attributes type
    fn new() -> Self {
        Self {
            divisions: 24,
            volume: 80,
            tempo: 108,
            key: 0,
            beats: 4,
            beat_type: 4,
            clef: Clef::G,
        }
    }

    /// Parses the tags and values inside of the "attributes" tag, returning a number of Attribute
    /// structures equal to the number of staves present or the number provided by the caller,
    /// whichever is higher
    ///
    /// # Arguments
    ///
    /// * 'parser' - A mutable reference to the parser located inside the "attributes" tag
    /// * 'attribute_list' - a mutable vector of attributes to use as a baseline
    ///
    fn parse_attributes(parser: &mut EventReader<BufReader<File>>, mut attribute_list: Vec<Self>) -> Vec<Self> {
        if attribute_list.is_empty() {
            attribute_list.push(Self::new());
        }
        loop {
            match parser.next() {
                Ok(XmlEvent::StartElement {name, attributes, ..}) => {
                    match name.local_name.as_str() {
                        "divisions" => {
                            let divisions: u32 = parse_tag_value("divisions", parser).parse::<u32>().unwrap();
                            for i in 0..attribute_list.len() {
                                attribute_list[i].divisions = divisions;
                            }
                        }
                        "key" => {
                            loop {
                                match parser.next() {
                                    Ok(XmlEvent::StartElement{name,..}) => {
                                        match name.local_name.as_str() {
                                            "fifths" => {
                                                let key: i32 = parse_tag_value("fifths", parser).parse::<i32>().unwrap();
                                                for i in 0..attribute_list.len() {
                                                    attribute_list[i].key = key;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    Ok(XmlEvent::EndElement{name}) => {
                                        if name.local_name.as_str() == "key" {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "time" => {
                            loop {
                                match parser.next() {
                                    Ok(XmlEvent::StartElement{name, ..}) => {
                                        match name.local_name.as_str() {
                                            "beats" => {
                                                let beats: u8 = parse_tag_value("beats", parser).parse::<u8>().unwrap();
                                                for i in 0..attribute_list.len() {
                                                    attribute_list[i].beats = beats;
                                                }
                                            }
                                            "beat-type" => {
                                                let beat_type: u8 = parse_tag_value("beat-type", parser).parse::<u8>().unwrap();
                                                for i in 0..attribute_list.len() {
                                                    attribute_list[i].beat_type = beat_type;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    Ok(XmlEvent::EndElement{name}) => {
                                        if name.local_name.as_str() == "time" {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "staves" => {
                            let staves = parse_tag_value("staves", parser).parse::<u8>().unwrap();
                            // Don't add extra attribute sets unless number of staves is >= 2
                            for i in 1..staves {
                                if i as usize >= attribute_list.len() {
                                    let next_attr = attribute_list[0].clone();
                                    attribute_list.push(next_attr);
                                }
                            }
                        }
                        "clef" => {
                            // Assume this refers to the first staff unless otherwise specified
                            let mut index = 1;
                            // In a single staff Part there are no attributes to the clef tag
                            if !attributes.is_empty() {
                                for attr in attributes {
                                    if attr.name.local_name.as_str() == "number" {
                                        index = attr.value.parse().unwrap();
                                    }
                                }
                            }
                            loop {
                                match parser.next() {
                                    Ok(XmlEvent::StartElement {name, ..}) => {
                                        if name.local_name.as_str() == "sign" {
                                            match parse_tag_value("sign", parser).as_str() {
                                                "G" => {
                                                    attribute_list[index - 1].clef = Clef::G;
                                                }
                                                "F" => {
                                                    attribute_list[index - 1].clef = Clef::F;
                                                }
                                                _ => {println!("Unrecognized Clef value");}
                                            }
                                        }
                                    }
                                    Ok(XmlEvent::EndElement {name}) => {
                                        if name.local_name.as_str() == "clef" {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(XmlEvent::EndElement {name, ..}) => {
                    if name.local_name.as_str() == "attributes" {
                        break;
                    }
                }
                _ => {}
            }
        }
        attribute_list
    }
}

/// A collection of Chords and a set of Attributes that represent a single Measure of a single Part
#[derive(Clone, Debug)]
struct Measure {
    chords: Vec<Chord>,
    attributes: Attributes,
}

impl Measure {
    /// Returns an instance of a measure using a set of Attributes as the base
    ///
    /// # Arguments
    ///
    /// * 'attr' - the Attributes structure to use in the measure
    ///
    fn from_attributes(attr: Attributes) -> Self {
        Self {
            chords: Vec::<Chord>::new(),
            attributes: attr,
        }
    }

    /// Parse a MusicXml measure and return a list of single staff measures
    ///
    /// # Arguments
    ///
    /// * 'parser'  - A mutable reference to the parser located inside the "measure" tag
    /// * 'attrs'   - A list of Attributes to use as the base attributes of any parsed measures
    ///
    fn parse_measure(parser: &mut EventReader<BufReader<File>>, attrs: Vec<Attributes>) -> Vec<Self> {
        let mut measures: Vec<Self> = Vec::<Self>::new();
        // Use a BTreeMap to group notes by start location and also sort chords by start location
        let mut note_map: BTreeMap<u32, Vec<Note>> = BTreeMap::new();
        let mut current_position: u32 = 0;
        let mut last_position: u32 = 0;

        // Clone so we're not borrowing the moved attr
        for attr in attrs.clone() {
            measures.push(Measure::from_attributes(attr));
        }
        loop {
            match parser.next() {
                Ok(XmlEvent::StartElement {name, ..}) => {
                    match name.local_name.as_str() {
                        "attributes" => {
                            let tmp_attributes = Attributes::parse_attributes(parser, attrs.clone());
                            // Attributes will tell us how many staves we have, make a measure for
                            // each one
                            if measures.len() < tmp_attributes.len() {
                                for i in 0.. measures.len() {
                                    measures[i].attributes = tmp_attributes[i].clone();
                                }
                                for i in measures.len()..tmp_attributes.len() {
                                    measures.push(Measure::from_attributes(tmp_attributes[i].clone()));
                                }
                            } else {
                                for i in 0..tmp_attributes.len() {
                                    measures[i].attributes = tmp_attributes[i].clone();
                                }
                            }
                        }
                        "note" => {
                            let (tmp_note, is_chord) = Note::parse_note(parser);
                            // Assume position will be current_position
                            let mut position = current_position;
                            if is_chord {
                                // If it's part of a chord just put it in the last position
                                position = last_position;
                                // current_position won't change unless we have different durations
                                // in the same chord, in which case use the smaller duration
                                if tmp_note.duration < (current_position - last_position) {
                                    current_position = last_position + tmp_note.duration;
                                }
                            } else {
                                last_position = current_position;
                                current_position += tmp_note.duration;
                            }
                            if let Some(notes) = note_map.get_mut(&position) {
                                notes.push(tmp_note);
                            } else {
                                note_map.insert(position, vec![tmp_note]);
                            }
                        }
                        "backup" => {
                            // Backup allows for changing the current_position without using chord
                            // tags
                            loop {
                                match parser.next() {
                                    Ok(XmlEvent::StartElement {name, ..}) => {
                                        if name.local_name.as_str() == "duration" {
                                            let tmp_duration = parse_tag_value("duration", parser).parse::<u32>().unwrap();
                                            if current_position >= tmp_duration {
                                                current_position -= tmp_duration;
                                            } else {
                                                current_position = 0;
                                            }
                                        }
                                    }
                                    Ok(XmlEvent::EndElement {name}) => {
                                        if name.local_name.as_str() == "backup" {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "direction" => {
                            loop {
                                match parser.next() {
                                    Ok(XmlEvent::StartElement {name, attributes, ..}) => {
                                        if name.local_name.as_str() == "sound" {
                                            for attr in attributes {
                                                match attr.name.local_name.as_str() {
                                                    "dynamics" => {
                                                        let vol = attr.value.parse::<f64>().unwrap().round() as u32;
                                                        for i in 0..measures.len() {
                                                            measures[i].attributes.volume = vol;
                                                        }
                                                    }
                                                    "tempo" => {
                                                        let tempo = attr.value.parse::<f64>().unwrap().round() as u32;
                                                        for i in 0..measures.len() {
                                                            measures[i].attributes.tempo = tempo;
                                                        }
                                                    }
                                                    // Direction has more tags but they are
                                                    // normally for visual formatting
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                    Ok(XmlEvent::EndElement {name}) => {
                                        if name.local_name.as_str() == "direction" {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(XmlEvent::EndElement {name, ..}) => {
                    if name.local_name.as_str() == "measure" {
                        // To finish parsing measures, turn the collection of notes into chords and
                        // save those chords to their respective measures based on staff #
                        let mut chords: Vec<Vec<Chord>> = vec![Vec::<Chord>::new()];
                        // Create a list of chords for each staff
                        for _i in 1..measures.len() {
                            chords.push(Vec::<Chord>::new());
                        }
                        for (start, note_vec) in note_map {
                            for note in note_vec {
                                let staff = note.staff;
                                // Check for existing chords on this staff
                                if let Some(last_chord) = chords[(staff - 1) as usize].last_mut() {
                                    // Check most recent chord on this staff to update if possible
                                    if last_chord.start_time != start {
                                        let mut tmp_chord = Chord::new();
                                        tmp_chord.start_time = start;
                                        tmp_chord.duration = note.duration;
                                        tmp_chord.note_type = note.note_type;
                                        tmp_chord.dotted = note.dotted;
                                        tmp_chord.is_rest = note.is_rest;
                                        tmp_chord.arpeggiate = note.arpeggiate;
                                        tmp_chord.triplet = note.triplet;
                                        tmp_chord.slur_start = note.slur_start;
                                        tmp_chord.slur_stop = note.slur_stop;
                                        tmp_chord.notes.push(note);
                                        chords[(staff - 1) as usize].push(tmp_chord);
                                    } else {
                                        if last_chord.duration > note.duration {
                                            last_chord.duration = note.duration;
                                            last_chord.note_type = note.note_type;
                                            last_chord.dotted = note.dotted;
                                        }
                                        last_chord.notes.push(note);
                                    }
                                } else {
                                    let mut tmp_chord = Chord::new();
                                    tmp_chord.start_time = start;
                                    tmp_chord.duration = note.duration;
                                    tmp_chord.note_type = note.note_type;
                                    tmp_chord.dotted = note.dotted;
                                    tmp_chord.is_rest = note.is_rest;
                                    tmp_chord.arpeggiate = note.arpeggiate;
                                    tmp_chord.triplet = note.triplet;
                                    tmp_chord.slur_start = note.slur_start;
                                    tmp_chord.slur_stop = note.slur_stop;
                                    tmp_chord.notes.push(note);
                                    chords[(staff - 1) as usize].push(tmp_chord);
                                }
                            }
                        }
                        for i in 0..measures.len() {
                            measures[i].chords.append(&mut chords[i]);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
        measures
    }

    /// Get the gjm duration value of a measure
    fn get_duration_max(&self) -> u32 {
        // To convert to gjm we get the ratio of the combined musicXml durations of all chords in
        // the measure over the theoretical expected duration of a full measure with the given time
        // signature and divisions. This lets us calculate the gjm duration as a ratio of the theoretical max.
        let mxml_max_dur = self.attributes.divisions * self.attributes.beats as u32;
        let gjm_max_dur = (64 / self.attributes.beat_type) * self.attributes.beats;
        let mut mxml_actual_dur = 0;
        for chord in self.chords.iter() {
            mxml_actual_dur += chord.duration;
        }
        let mxml_dur_ratio = mxml_actual_dur as f64 / mxml_max_dur as f64;
        // Subtract one because gjm expects the max start duration minus the minimum note length.
        let mut duration_max = (mxml_dur_ratio * gjm_max_dur as f64).round() as u32;
        if duration_max > 0 {
            duration_max -= 1;
        }
        duration_max
    }

    fn get_duration_ratio(&self) -> f64 {
        let mxml_max_dur = self.attributes.divisions * self.attributes.beats as u32;
        let gjm_max_dur = (64 / self.attributes.beat_type) * self.attributes.beats;
        gjm_max_dur as f64 / mxml_max_dur as f64
    }
}

/// A collection of sets of measures that are considered the same Part by MusicXml but exist on different
/// staves, requiring they be treated as seperate by GJM
#[derive(Debug)]
struct Part {
    measures: Vec<Vec<Measure>>,
}

impl Part {
    /// Returns a default instantiation of a Part
    fn new() -> Self {
        Self {
            measures: vec![Vec::<Measure>::new()],
        }
    }

    /// Parses the tags and values inside of a "part" tag and returns a single part that may have
    /// multiple parts by GJM standards
    fn parse_part(parser: &mut EventReader<BufReader<File>>) -> Self {
        let mut part = Part::new();
        loop {
            match parser.next() {
                Ok(XmlEvent::StartElement {name, ..}) => {
                    match name.local_name.as_str() {
                        "measure" => {
                            // Attributes carry over from one measure to the next if available
                            let mut attrs = Vec::<Attributes>::new();
                            for i in 0..part.measures.len() {
                                if part.measures[i].len() > 0 {
                                    attrs.push(part.measures[i].last().unwrap().attributes.clone());
                                } else {
                                    attrs.push(Attributes::new());
                                }
                            }
                            let tmp_measures = Measure::parse_measure(parser, attrs);
                            for i in 0..tmp_measures.len() {
                                if tmp_measures.len() > part.measures.len() {
                                    part.measures.push(Vec::<Measure>::new());
                                }
                                part.measures[i].push(tmp_measures[i].clone());
                            }
                        }
                        _ => {}
                    }
                }
                Ok(XmlEvent::EndElement {name, ..}) => {
                    if name.local_name.as_str() == "part" {
                        break;
                    }
                }
                _ => {}
            }
        }
        part
    }

    fn write_part_gjn(&self, file: &mut File, part_idx: &mut usize) -> std::io::Result<()> {
        for part in self.measures.iter() {
            if *part_idx < MAX_PART_COUNT {
                let line = format!("{}[{}] = {{\n", indent(1), part_idx);
                file.write_all(line.as_bytes())?;

                let (keys, clefs, volumes) = calc_measure_maps(part);

                // Key Signature Map
                let line = format!("{}MeasureKeySignatureMap = {{\n", indent(2));
                file.write_all(line.as_bytes())?;
                for (i, key) in keys {
                    let line = format!("{}{{ {}, {} }},\n", indent(3), i, key);
                    file.write_all(line.as_bytes())?;
                }
                let line = format!("{}}},\n", indent(2));
                file.write_all(line.as_bytes())?;

                // Clef Type Map
                let line = format!("{}MeasureClefTypeMap = {{\n", indent(2));
                file.write_all(line.as_bytes())?;
                for (i, clef) in clefs {
                    let clef_str;
                    match clef {
                        Clef::F => {
                            clef_str = "L4F";
                        }
                        Clef::G => {
                            clef_str = "L2G";
                        }
                    }
                    let line = format!("{}{{ {}, '{}' }},\n", indent(3), i, clef_str);
                    file.write_all(line.as_bytes())?;
                }
                let line = format!("{}}},\n", indent(2));
                file.write_all(line.as_bytes())?;
                
                // Hardcoded Maps
                    // Instrument
                let line = format!("{}MeasureInstrumentTypeMap = {{\n", indent(2));
                file.write_all(line.as_bytes())?;
                let line = format!("{}{{ 0, 'Piano' }},\n", indent(3));
                file.write_all(line.as_bytes())?;
                let line = format!("{}}},\n", indent(2));
                file.write_all(line.as_bytes())?;
                    // Volume Curve
                let line = format!("{}MeasureVolumeCurveMap = {{\n", indent(2));
                file.write_all(line.as_bytes())?;
                let line = format!("{}{{ 0, {{0.8, 0.7, 0.5, 0.5, 0.7, 0.6, 0.5, 0.4}} }},\n", indent(3));
                file.write_all(line.as_bytes())?;
                let line = format!("{}}},\n", indent(2));
                file.write_all(line.as_bytes())?;

                // Volume Map
                let line = format!("{}MeasureVolumeMap = {{\n", indent(2));
                file.write_all(line.as_bytes())?;
                for (i, volume) in volumes {
                    let line = format!("{}{{ {}, {} }},\n", indent(3), i, volume);
                    file.write_all(line.as_bytes())?;
                }
                let line = format!("{}}},\n", indent(2));
                file.write_all(line.as_bytes())?;

                for (i, measure) in part.iter().enumerate() {
                    // Measure index
                    let line = format!("{}[{}] = {{\n", indent(2), i);
                    file.write_all(line.as_bytes())?;

                    // Duration of measure (expressed as divisions)
                    let line = format!("{}DurationStampMax = {},\n", indent(3), measure.get_duration_max());
                    file.write_all(line.as_bytes())?;

                    // Number of notes (chords really)
                    let line = format!("{}NotePackCount = {},\n", indent(3), measure.chords.len());
                    file.write_all(line.as_bytes())?;

                    let mut current_dur = 0;
                    for (j, chord) in measure.chords.iter().enumerate() {
                        // Chord index
                        let line = format!("{}[{}] = {{\n", indent(3), j);
                        file.write_all(line.as_bytes())?;

                        // Add a line if chord is a rest and set notecount to zero for that chord
                        let mut note_count = chord.notes.len();
                        if chord.is_rest {
                            let line = format!("{}IsRest = true,\n", indent(4));
                            file.write_all(line.as_bytes())?;
                            note_count = 0;
                        }

                        // Add ties/slurs
                        if chord.slur_start && chord.slur_stop {
                            let line = format!("{}TieType ='Both',\n", indent(4));
                            file.write_all(line.as_bytes())?;
                        } else if chord.slur_start {
                            let line = format!("{}TieType ='Start',\n", indent(4));
                            file.write_all(line.as_bytes())?;
                        } else if chord.slur_stop {
                            let line = format!("{}TieType ='End',\n", indent(4));
                            file.write_all(line.as_bytes())?;
                        }

                        // Add a line if chord is dotted
                        if chord.dotted {
                            let line = format!("{}IsDotted = true,\n", indent(4));
                            file.write_all(line.as_bytes())?;
                        }

                        // Triplet if appropriate (any tuple is a triplet for now)
                        if chord.triplet {
                            let line = format!("{}Triplet = true,\n", indent(4));
                            file.write_all(line.as_bytes())?;
                        }

                        // Duration type is just string version of note type
                        let line = format!("{}DurationType = '{}',\n", indent(4), chord.gjm_note_string());
                        file.write_all(line.as_bytes())?;
                        
                        // Arpeggiate if appropriate (always up for now)
                        if chord.arpeggiate {
                            let line = format!("{}ArpeggioMode ='Upward',\n", indent(4));
                            file.write_all(line.as_bytes())?;
                        }

                        let line = format!("{}StampIndex = {},\n", indent(4), current_dur);
                        file.write_all(line.as_bytes())?;
                        let duration_ratio = measure.get_duration_ratio();
                        current_dur += chord.gjm_duration(duration_ratio);

                        // PitchSignCount is just how many notes are in the chord
                        let line = format!("{}ClassicPitchSignCount = {},\n", indent(4), note_count);
                        file.write_all(line.as_bytes())?;

                        if note_count > 0 {
                            let line = format!("{}ClassicPitchSign = {{\n", indent(4));
                            file.write_all(line.as_bytes())?;
                            for note in chord.notes.iter() {
                                let line = format!("{}[{}] = {{ NumberedSign = {}, PlayingPitchIndex = {}, AlterantType = '{}', RawAlterantType = '{}', }},\n",
                                    indent(5),
                                    note.pitch_index,
                                    note.get_numbered_sign(),
                                    note.pitch_index as i32 + note.alter,
                                    note.get_alterant_type(),
                                    note.get_alterant_type(),
                                );
                                file.write_all(line.as_bytes())?;
                            }
                            let line = format!("{}}},\n", indent(4));
                            file.write_all(line.as_bytes())?;
                        }

                        // Close the chord
                        let line = format!("{}}},\n", indent(3));
                        file.write_all(line.as_bytes())?;
                    }
                    // Close the measure
                    let line = format!("{}}},\n", indent(2));
                    file.write_all(line.as_bytes())?;
                }

                // Close the part
                let line = format!("{}}},\n", indent(1));
                file.write_all(line.as_bytes())?;
            }

            *part_idx += 1;
        }
        Ok(())
    }
}

/// A collection of parts
#[derive(Debug)]
pub struct Score {
    parts: Vec<Part>,
}

impl Score {
    /// Returns a default instantiation of a Score
    pub fn new() -> Self {
        Self {parts: Vec::<Part>::new()}
    }

    /// Parses the tags and values of an entire partwise score
    pub fn parse_score(parser: &mut EventReader<BufReader<File>>) -> Self {
        let mut score = Score::new();
        loop {
            match parser.next() {
                Ok(XmlEvent::StartElement {name, ..}) => {
                    match name.local_name.as_str() {
                        "part" => {
                            score.parts.push(Part::parse_part(parser));
                        }
                        _ => {}
                    }
                }
                Ok(XmlEvent::EndElement {name, ..}) => {
                    if name.local_name.as_str() == "score-partwise" {
                        break;
                    }
                }
                _ => {}
            }
        }

        score
    }

    pub fn write_score_gjn(&self, file: &mut File) -> std::io::Result<()> {
        file.write_all(b"Notation.RegularTracks = {\n")?;
        
        let mut part_idx = 0;
        for part in self.parts.iter() {
            part.write_part_gjn(file, &mut part_idx)?;
        }

        file.write_all(b"}")?;
        Ok(())
    }

    pub fn get_beats_per_measure(&self) -> u8 {
        self.parts[0].measures[0][0].attributes.beats
    }

    pub fn get_beat_duration_type(&self) -> u8 {
        self.parts[0].measures[0][0].attributes.beat_type
    }

    pub fn get_bpm_map(&self) -> String {
        let mut map = String::new();

        let mut tempo = 0;
        for (i, measure) in self.parts[0].measures[0].iter().enumerate() {
            if measure.attributes.tempo != tempo {
                write!(&mut map, "\t\t{{ {}, {} }},\n", i, measure.attributes.tempo).unwrap();
                tempo = measure.attributes.tempo;
            }
        }
        map
    }

    pub fn get_measure_count(&self) -> usize {
        self.parts[0].measures[0].len()
    }
}

