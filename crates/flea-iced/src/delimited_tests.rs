use super::*;

fn c(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn records(s: &str, d: char) -> Vec<Vec<String>> {
    let t = c(s);
    let starts = record_starts(&t, d, 1000);
    (0..starts.len() - 1).map(|r| fields(&t, starts[r], starts[r + 1], d)).collect()
}

#[test]
fn quoted_fields_hold_delimiters_quotes_and_newlines() {
    assert_eq!(records("a,\"b,c\",\"say \"\"hi\"\"\"\r\nx,\"two\nlines\",z\n", ','), vec![
        vec!["a".to_string(), "b,c".into(), "say \"hi\"".into()],
        vec!["x".to_string(), "two\nlines".into(), "z".into()],
    ]);
}

#[test]
fn a_quote_inside_a_field_does_not_open_one() {
    assert_eq!(records("5\" pipe,ok\nnext,row\n", ','), vec![vec!["5\" pipe".to_string(), "ok".into()], vec!["next".to_string(), "row".into()]]);
}

#[test]
fn a_bom_is_skipped_and_an_unclosed_quote_runs_to_the_end() {
    assert_eq!(records("\u{FEFF}h1,h2\n", ','), vec![vec!["h1".to_string(), "h2".into()]]);
    assert_eq!(records("a,\"open\nstill\n", ',').len(), 1);
}

#[test]
fn the_delimiter_is_sniffed_unless_the_extension_names_it() {
    assert_eq!(sniff(&c("a;b;c\n1;2;3\n4;5;6\n"), "x.csv"), ';');
    assert_eq!(sniff(&c("a,b\n1,2\n"), "x.tsv"), '\t');
    assert_eq!(sniff(&c("a|b|c\n1|2|3\n"), "x.txt"), '|');
    // Nothing agrees on a width above one: the comma is the default.
    assert_eq!(sniff(&c("one\ntwo\n"), "x.csv"), ',');
}

#[test]
fn widths_count_the_first_line_of_a_cell_and_numbers_are_recognised_by_column() {
    let t = c("name,amount\nwidget,\"1,234.50\"\nlong name,-7\nnote,\"multi\nline\"\n");
    let starts = record_starts(&t, ',', 1000);
    assert_eq!(column_chars(&t, &starts, ',', 40), vec![9, 8]);
    assert_eq!(numeric_columns(&t, &starts[..4], ','), vec![false, true]);
    assert_eq!(shown("a\r\nb"), "a ↵");
    // Every case's answer is the JS regex's own, taken with node on this box.
    for n in ["1", "-1.5", "1,234", "1 234,5", "12%", "3e-7", "+4", "1,23", "1.234.567,89", "12,3456", "1234,5", "1,234.567.890", "1'000", "00012,5e+3%", "1,2345"] {
        assert!(is_number(n), "{n}");
    }
    for n in ["abc", "1.2.3x", "", "e5", "12 34", ".5", "5.", "1,,234"] {
        assert!(!is_number(n), "{n}");
    }
}
