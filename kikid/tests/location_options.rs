//! A select field's value saved as its label (every location before 0.2.0) is read as the value
//! it means, and the file is rewritten once at the daemon's start.

mod common;

use kikid::json::Value;
use kikid::locations;

#[test]
fn a_value_saved_as_its_label_is_read_as_the_value_and_rewritten_once() {
    let dir = common::setup("location-options");
    // Saved the old way: the option's label where its value goes.
    let loc = Value::obj().s("name", "lab").s("plugin", "stub").s("remoteUri", "stub://lab/").v("config", Value::obj().s("name", "lab").s("flavour", "Hot").done()).done();
    locations::save(loc, &Value::obj().done(), None, true).unwrap();
    let file = dir.join("config/locations.toml");
    assert!(std::fs::read_to_string(&file).unwrap().contains("flavour = \"Hot\""), "the file holds the label, as an old one would");

    // Read as the value, by the stub's own form — without touching the file.
    let read = locations::find("lab").unwrap();
    assert_eq!(read.get("config").unwrap().str_field("flavour"), Some("hot"));
    assert!(std::fs::read_to_string(&file).unwrap().contains("flavour = \"Hot\""), "reading does not write");

    // Rewritten once, at start; a value already right is left alone.
    locations::migrate_option_labels();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("flavour = \"hot\"") && !text.contains("\"Hot\""), "{text}");
    locations::migrate_option_labels();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "nothing to do the second time");

    // A value the form does not know stays as it is: not ours to guess.
    let odd = Value::obj().s("name", "odd").s("plugin", "stub").s("remoteUri", "stub://odd/").v("config", Value::obj().s("name", "odd").s("flavour", "smoky").done()).done();
    locations::save(odd, &Value::obj().done(), None, true).unwrap();
    assert_eq!(locations::find("odd").unwrap().get("config").unwrap().str_field("flavour"), Some("smoky"));
    let _ = std::fs::remove_dir_all(&dir);
}
