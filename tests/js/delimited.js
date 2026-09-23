.import "../../ui/js/Delimited.js" as Delimited

// Every record of a body, as fields joined with | and records joined with /, for one-line checks.
function table(text, delim) {
    var starts = Delimited.recordStarts(text, delim, 1000)
    var out = []
    for (var i = 0; i + 1 < starts.length; i++) out.push(Delimited.fields(text, starts[i], starts[i + 1], delim).join("|"))
    return out.join("/")
}

function run(check) {
    check("an empty body is no records", Delimited.recordStarts("", ",", 10).length - 1, 0)
    check("plain records split on the separator", table("a,b,c\n1,2,3\n", ","), "a|b|c/1|2|3")
    check("a last record with no newline is still a record", table("a,b\n1,2", ","), "a|b/1|2")
    check("CRLF line endings leave no carriage return in the last field", table("a,b\r\n1,2\r\n", ","), "a|b/1|2")
    check("a byte order mark is not part of the first header", table("﻿name,age\nx,1\n", ","), "name|age/x|1")
    check("empty fields are kept, trailing ones too", table("a,,c,\n", ","), "a||c|")

    // The line break inside a quoted field is data, so it does not end the record.
    check("a quoted field keeps its separator", table("\"Smith, J\",42\n", ","), "Smith, J|42")
    check("a quoted field keeps its line break, and the record goes on",
          table("id,note\n1,\"first line\nsecond line\"\n2,plain\n", ","), "id|note/1|first line\nsecond line/2|plain")
    check("a doubled quote is one quote", table("\"say \"\"hi\"\"\",x\n", ","), "say \"hi\"|x")
    check("a doubled quote next to a line break does not end the field early",
          table("\"a\"\"\nb\",c\nd,e\n", ","), "a\"\nb|c/d|e")
    check("a quote mid-field is an inch mark, not the start of a quoted field",
          table("tv,5\" screen\nradio,none\n", ","), "tv|5\" screen/radio|none")
    check("an unterminated quote swallows the rest rather than inventing records",
          table("a,\"open\nb,c\n", ","), "a|open\nb,c")
    check("the record limit stops the walk and the end offset marks where",
          Delimited.recordStarts("a\nb\nc\nd\n", ",", 2).join(","), "0,2,4")

    check("a .tsv and a .tab are tabs, a .psv is pipes, whatever the body holds",
          [Delimited.sniff("a,b\n", "x.tsv"), Delimited.sniff("a,b\n", "x.TAB"), Delimited.sniff("a,b\n", "x.psv")]
              .map(function (d) { return d === "\t" ? "tab" : d }).join(" "), "tab tab |")
    check("a .csv written with semicolons is sniffed as semicolons",
          Delimited.sniff("name;price\nwidget;1,50\ngadget;2,75\n", "prices.csv"), ";")
    check("a comma inside quotes does not outvote the real separator",
          Delimited.sniff("\"a,b,c\";\"d,e\"\n\"f,g\";h\n", "q.csv"), ";")
    check("a tab separated .csv is sniffed as tabs",
          Delimited.sniff("a\tb\tc\n1\t2\t3\n", "x.csv"), "\t")
    check("a body nothing splits falls back to a comma", Delimited.sniff("one\ntwo\n", "x.csv"), ",")

    var body = "id,name\n1,\"a much longer name\"\n22,b\n"
    check("column widths are the widest field in characters, capped",
          Delimited.columnChars(body, Delimited.recordStarts(body, ",", 100), ",", 10).join(","), "2,10")
    var mixed = "id,name,price,when\n1,a,\"1,299.50\",2026-09-23\n2,b,-3e5,\n3,c,,2026-09-24\n"
    check("a column of numbers below its header is numeric, and a date or a name is not",
          Delimited.numericColumns(mixed, Delimited.recordStarts(mixed, ",", 100), ",").join(","), "true,false,true,false")
    check("a European decimal comma and a percentage are numbers too",
          Delimited.numericColumns("x;y\n1,50;12%\n", Delimited.recordStarts("x;y\n1,50;12%\n", ";", 10), ";").join(","), "true,true")
    var tall = "a,b\n\"four\nmore text here\",x\n"
    check("a multi-line cell is as wide as its first line and its return mark, not its longest line",
          Delimited.columnChars(tall, Delimited.recordStarts(tall, ",", 10), ",", 40).join(","), "6,1")
    check("a multi-line cell draws its first line and a return mark",
          Delimited.shown("first\r\nsecond") + "|" + Delimited.shown("one"), "first ↵|one")
}
