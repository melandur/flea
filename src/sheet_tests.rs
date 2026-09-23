use super::*;
use crate::backend::testdir::TestDir;
use crate::sheetxml::{attr, decode, tokens, Tok};
use crate::sheetxlsx::{column_of, is_date_code, number, serial_date, Book};

// Sample input, trimmed from an Excel 365 save: two shared strings, one of them rich text with a
// phonetic guide that must not show, and a date style at index 1.
const STRINGS: &str = r#"<?xml version="1.0"?><sst xmlns="x" count="3"><si><t>Name</t></si><si><r><rPr/><t xml:space="preserve">Zürich </t></r><r><t>&amp; Co</t></r><rPh><t>ignored</t></rPh></si><si/></sst>"#;
const STYLES: &str = r#"<styleSheet><numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy\-mm\-dd"/></numFmts><cellStyleXfs><xf numFmtId="14"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="164"/><xf numFmtId="4"/></cellXfs></styleSheet>"#;
const WORKBOOK: &str = r#"<workbook xmlns:r="rel"><workbookPr/><sheets><sheet name="Data" sheetId="7" r:id="rId3"/><sheet name="Other" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
const RELS: &str = r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId3" Target="worksheets/sheet3.xml"/></Relationships>"#;
const SHEET: &str = r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="C1" t="inlineStr"><is><t>When</t></is></c></row><row r="3"><c r="A3" t="s"><v>1</v></c><c r="B3"><v>0.30000000000000004</v></c><c r="C3" s="1"><v>45923</v></c><c r="D3" t="b"><v>1</v></c><c r="E3" t="str"><f>A1</f><v>x, "y"</v></c><c r="F3" t="e"><v>#DIV/0!</v></c></row></sheetData></worksheet>"#;

fn book() -> Book {
    Book { strings: sheetxlsx::shared_strings(STRINGS), date_styles: sheetxlsx::date_styles(STYLES), date1904: false }
}

#[test]
fn the_tokenizer_drops_prefixes_decodes_entities_and_skips_what_no_cell_shows() {
    let names: Vec<String> = tokens("<?xml?><!-- c --><x:row r='1'><c/></x:row>text<![CDATA[<raw>]]>").map(|t| match t {
        Tok::Open { name, empty, .. } => format!("open {}{}", name, if empty { "/" } else { "" }),
        Tok::Close(n) => format!("close {}", n),
        Tok::Text(s) => format!("text {}", s),
        Tok::Cdata(s) => format!("cdata {}", s),
    }).collect();
    assert_eq!(names, ["open row", "open c/", "close row", "text text", "cdata <raw>"]);
    assert_eq!(attr(r#" table:number-columns-repeated="3" b='a &amp; b'"#, "number-columns-repeated").as_deref(), Some("3"));
    assert_eq!(attr(r#" b='a &amp; b'"#, "b").as_deref(), Some("a & b"));
    assert_eq!(attr(r#" sheetId="7" r:id="rId3""#, "id").as_deref(), Some("rId3"), "sheetId is not id");
    assert_eq!(decode("&lt;&#65;&#x42;&bogus;&"), "<AB&bogus;&");
}

#[test]
fn shared_strings_join_rich_runs_and_leave_out_the_phonetic_guide() {
    assert_eq!(sheetxlsx::shared_strings(STRINGS), ["Name", "Zürich & Co", ""]);
}

#[test]
fn only_cell_formats_decide_dates_and_a_custom_code_is_read_for_its_letters() {
    assert_eq!(sheetxlsx::date_styles(STYLES), [false, true, false], "cellStyleXfs is not what s= indexes");
    assert!(is_date_code("dd/mm/yyyy hh:mm") && is_date_code("[$-409]mmm d") && is_date_code("[h]:mm:ss"));
    assert!(!is_date_code("General") && !is_date_code("0.00") && !is_date_code(r#"#,##0 "days""#) && !is_date_code("[Red]0.0"));
}

#[test]
fn the_first_sheet_is_the_first_tab_through_its_relationship_not_sheet1() {
    assert_eq!(sheetxlsx::first_sheet(WORKBOOK, RELS), "xl/worksheets/sheet3.xml");
    assert_eq!(sheetxlsx::first_sheet("<workbook/>", ""), "xl/worksheets/sheet1.xml");
    assert_eq!(sheetxlsx::first_sheet(WORKBOOK, r#"<Relationship Id="rId3" Target="/xl/ws/a.xml"/>"#), "xl/ws/a.xml");
    assert!(!sheetxlsx::date1904(WORKBOOK) && sheetxlsx::date1904(r#"<workbookPr date1904="1"/>"#));
}

#[test]
fn cells_land_in_their_own_columns_and_a_skipped_row_stays_a_row() {
    let rows = sheetxlsx::rows(SHEET, &book(), 100, 256);
    assert_eq!(rows[0], ["Name", "", "When"]);
    assert!(rows[1].is_empty(), "row 2 is not in the file and is still row 2");
    assert_eq!(rows[2], ["Zürich & Co", "0.3", "2025-09-23", "TRUE", "x, \"y\"", "#DIV/0!"]);
    assert_eq!(sheetxlsx::rows(SHEET, &book(), 1, 256).len(), 1, "the limit is rows, not cells");
    assert_eq!(sheetxlsx::rows(SHEET, &book(), 100, 2)[0], ["Name"], "columns past the cap are not written");
    let styled = SHEET.replace("</sheetData>", r#"<row r="9"><c r="A9" s="1"/></row><row r="1000" s="2"></row></sheetData>"#);
    assert_eq!(sheetxlsx::rows(&styled, &book(), 2000, 256).len(), 3, "formatted empty rows at the end are not records");
}

#[test]
fn references_serials_and_numbers_read_the_way_excel_shows_them() {
    assert_eq!((column_of("A1"), column_of("Z9"), column_of("AA10"), column_of("xfd1")), (0, 25, 26, 16383));
    assert_eq!(serial_date(1.0, false), "1900-01-01");
    assert_eq!(serial_date(61.0, false), "1900-03-01", "the 29 February 1900 Excel invented is skipped");
    assert_eq!(serial_date(45923.75, false), "2025-09-23 18:00");
    assert_eq!(serial_date(0.5, false), "12:00", "a time with no date");
    assert_eq!(serial_date(0.0, true), "1904-01-01");
    assert_eq!((number(3.0), number(-2.5), number(0.1 + 0.2), number(1e20)), ("3".into(), "-2.5".into(), "0.3".into(), "100000000000000000000".into()));
}

// Sample input: the shape LibreOffice 25 writes, with the trailing empty columns and rows it pads
// every sheet with, a covered cell, a comment and a repeated value.
const CONTENT: &str = r#"<office:document-content><office:body><office:spreadsheet><table:table table:name="One">
  <table:table-column table:number-columns-repeated="3"/>
  <table:table-row>
    <table:table-cell office:value-type="string"><text:p>Item</text:p></table:table-cell>
    <table:table-cell office:value-type="date" office:date-value="2026-09-23"><text:p>23/09/26</text:p></table:table-cell>
    <table:table-cell table:number-columns-repeated="1021"/>
  </table:table-row>
  <table:table-row table:number-rows-repeated="2"><table:table-cell table:number-columns-repeated="1024"/></table:table-row>
  <table:table-row>
    <table:table-cell office:value-type="float" office:value="2" table:number-columns-repeated="2"><text:p>2</text:p></table:table-cell>
    <table:covered-table-cell/>
    <table:table-cell><office:annotation><text:p>note</text:p></office:annotation><text:p>a<text:s text:c="2"/>b</text:p><text:p>c<text:tab/>d</text:p></table:table-cell>
  </table:table-row>
  <table:table-row table:number-rows-repeated="1048570"><table:table-cell table:number-columns-repeated="1024"/></table:table-row>
</table:table><table:table table:name="Two"><table:table-row><table:table-cell><text:p>no</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#;

#[test]
fn an_ods_table_shows_its_display_text_and_expands_repeats_only_where_something_follows() {
    let rows = sheetods::rows(CONTENT, 100, 256);
    assert_eq!(rows.len(), 4, "the padding rows at the end are not rows, got {:?}", rows);
    assert_eq!(rows[0], ["Item", "23/09/26"]);
    assert!(rows[1].is_empty() && rows[2].is_empty(), "the two empty rows before data are kept");
    assert_eq!(rows[3], ["2", "2", "", "a  b\nc\td"]);
    assert_eq!(sheetods::rows(CONTENT, 2, 256).len(), 2);
    assert_eq!(sheetods::rows(CONTENT, 100, 3)[3], ["2", "2", ""]);
}

#[test]
fn csv_quotes_only_what_needs_it_and_counts_closing_tags_across_chunks() {
    let rows = vec![vec!["a".to_string(), "b,c".into(), "say \"hi\"".into(), "two\nlines".into()], vec![]];
    assert_eq!(csv(&rows), "a,\"b,c\",\"say \"\"hi\"\"\",\"two\nlines\"\n\n");
    assert_eq!(closes(b"<row></row><x:row></x:row></rows></r", 0, "row"), (2, 33));
    assert_eq!(closes(b"</row><", 0, "row"), (1, 6), "a trailing < is kept for the next chunk");
    assert_eq!(format_of("Book.XLSX"), Some(Format::Xlsx));
    assert_eq!((format_of("a.ods"), format_of("a.fods"), format_of("a.xls")), (Some(Format::Ods), Some(Format::Fods), None));
}

fn zip(dir: &TestDir, name: &str, src: &str) -> std::path::PathBuf {
    let out = dir.join(name);
    let ok = Command::new(BSDTAR).args(["--format", "zip", "-cf"]).arg(&out).arg("-C").arg(dir.join(src)).arg(".")
        .status().map(|s| s.success()).unwrap_or(false);
    assert!(ok, "bsdtar could not build the fixture");
    out
}

#[test]
fn a_real_xlsx_and_ods_package_read_end_to_end_through_bsdtar() {
    let dir = TestDir::new("sheetpackage");
    dir.dir("x/xl/_rels");
    dir.dir("x/xl/worksheets");
    dir.file("x/xl/workbook.xml", WORKBOOK);
    dir.file("x/xl/_rels/workbook.xml.rels", RELS);
    dir.file("x/xl/sharedStrings.xml", STRINGS);
    dir.file("x/xl/styles.xml", STYLES);
    dir.file("x/xl/worksheets/sheet3.xml", SHEET);
    let xlsx = zip(&dir, "book.xlsx", "x");
    let rows = read(&xlsx, 100).expect("the package reads");
    assert_eq!(rows[2][2], "2025-09-23");
    dir.dir("o");
    dir.file("o/content.xml", CONTENT);
    let ods = zip(&dir, "book.ods", "o");
    assert_eq!(read(&ods, 100).expect("the package reads")[0], ["Item", "23/09/26"]);
    let fods = dir.file("flat.fods", CONTENT);
    assert_eq!(read(&fods, 100).unwrap().len(), 4);
    assert!(read(&dir.file("broken.xlsx", "not a zip"), 10).is_err(), "a file that is not a package is an error, not an empty sheet");
}
