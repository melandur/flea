// The colours syntax.rs's roles draw in, ui/js/SyntaxStyle.js: PyCharm's Monokai, the operator's
// ruling of 2026-10-08, each lifted to 4.5:1 on the ground it is drawn over and comments to 3:1, so
// a dark theme keeps Monokai's own hexes and a light one darkens them, hue kept, until they read.
// Keywords and parameters are italic, headings and strong text bold. The overlay's text editor is
// coloured through iced's line-by-line Highlighter, and the column's first lines as rich text.
use crate::syntax::{self, Role, Spec, State};
use crate::contrast::ensure_ratio;
use iced::advanced::text::highlighter::{self, Highlighter};
use iced::font::{Style, Weight};
use iced::widget::text::Span;
use iced::{Color, Font};
use std::ops::Range;

const MONOKAI: [(Role, u32); 10] = [
    (Role::Keyword, 0x66d9ef),
    (Role::Builtin, 0x66d9ef),
    (Role::String, 0xe6db74),
    (Role::Number, 0xae81ff),
    (Role::Comment, 0x75715e),
    (Role::Operator, 0xf92672),
    (Role::Func, 0xa6e22e),
    (Role::Param, 0xfd971f),
    (Role::Heading, 0xa6e22e),
    (Role::Strong, 0xfd971f),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colours([Color; 10]);

pub fn colours(ground: Color) -> Colours {
    Colours(MONOKAI.map(|(role, hex)| {
        let c = Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
        ensure_ratio(c, ground, if role == Role::Comment { 3.0 } else { 4.5 })
    }))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Highlight {
    pub color: Color,
    pub italic: bool,
    pub bold: bool,
}

impl Colours {
    pub fn of(&self, role: Role) -> Highlight {
        let at = MONOKAI.iter().position(|(r, _)| *r == role).unwrap_or(0);
        Highlight { color: self.0[at], italic: matches!(role, Role::Keyword | Role::Param), bold: matches!(role, Role::Heading | Role::Strong) }
    }
}

pub fn font(h: &Highlight) -> Font {
    Font {
        style: if h.italic { Style::Italic } else { Style::Normal },
        weight: if h.bold { Weight::Bold } else { Weight::Normal },
        ..Font::DEFAULT
    }
}

// The overlay's editor: the family and the colours on its surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub spec: &'static Spec,
    pub colours: Colours,
}

// Syntax.memo, as iced asks for it: the state each line starts in, kept, so a changed line is
// highlighted again from the state above it and nothing earlier is scanned twice. One difference
// from the JavaScript's 4 KB runs: a run carried a parameter list across its own line breaks, and a
// line carries only an open comment or string, so a list's later lines draw their names plain.
pub struct Monokai {
    settings: Settings,
    states: Vec<Option<State>>,
    current: usize,
}

impl Highlighter for Monokai {
    type Settings = Settings;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(settings: &Settings) -> Self {
        Monokai { settings: settings.clone(), states: vec![None], current: 0 }
    }

    fn update(&mut self, settings: &Settings) {
        *self = Self::new(settings);
    }

    fn change_line(&mut self, line: usize) {
        self.current = line.min(self.states.len() - 1);
        self.states.truncate(self.current + 1);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let state = self.states.get(self.current).cloned().flatten();
        let painted = syntax::highlight(line, self.settings.spec, state.as_ref());
        self.states.truncate(self.current + 1);
        self.states.push(painted.state);
        self.current += 1;
        let c = self.settings.colours;
        painted.spans.into_iter().map(|(r, role)| (r, c.of(role))).collect::<Vec<_>>().into_iter()
    }

    fn current_line(&self) -> usize {
        self.current
    }
}

pub fn format(h: &Highlight, _theme: &iced::Theme) -> highlighter::Format<Font> {
    highlighter::Format { color: Some(h.color), font: Some(font(h)) }
}

// Syntax.lines: the column's first lines, an open comment carried from one line to the next, tabs
// expanded to their stops because the column draws the file's own layout.
pub fn lines<'a>(lines: &[String], spec: &'static Spec, colours: Colours, plain: Color) -> Vec<Vec<Span<'a, (), Font>>> {
    let mut state = None;
    lines
        .iter()
        .map(|raw| {
            let line = syntax::expand_tabs(raw);
            let painted = syntax::highlight(&line, spec, state.as_ref());
            state = painted.state;
            let mut out = vec![];
            let mut at = 0;
            for (r, role) in painted.spans {
                if r.start > at {
                    out.push(Span::new(line[at..r.start].to_string()).color(plain));
                }
                let h = colours.of(role);
                out.push(Span::new(line[r.clone()].to_string()).color(h.color).font(font(&h)));
                at = r.end;
            }
            if at < line.len() {
                out.push(Span::new(line[at..].to_string()).color(plain));
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dark_ground_keeps_monokai_and_a_light_one_lifts_it() {
        let dark = colours(Color::from_rgb8(0x1e, 0x1e, 0x1e));
        assert_eq!(dark.of(Role::String).color, Color::from_rgb8(0xe6, 0xdb, 0x74));
        let light = colours(Color::WHITE);
        assert_ne!(light.of(Role::String).color, Color::from_rgb8(0xe6, 0xdb, 0x74));
        assert!(dark.of(Role::Keyword).italic && dark.of(Role::Heading).bold && !dark.of(Role::String).italic);
    }

    #[test]
    fn the_editor_highlighter_carries_a_comment_down_and_restarts_where_a_line_changed() {
        let spec = syntax::language("a.c").unwrap();
        let mut h = Monokai::new(&Settings { spec, colours: colours(Color::BLACK) });
        let first: Vec<_> = h.highlight_line("int a; /* open").collect();
        assert_eq!(first.len(), 2);
        let second: Vec<_> = h.highlight_line("still */ x").collect();
        assert_eq!(second[0].0, 0..8, "the comment carried down");
        h.change_line(0);
        assert_eq!(h.current_line(), 0);
        h.highlight_line("int a;").for_each(drop);
        let after: Vec<_> = h.highlight_line("still */ x").collect();
        assert!(after.iter().all(|(r, _)| *r != (0..8)), "with the opener gone the line is code again");
    }

    #[test]
    fn the_columns_lines_keep_every_byte_and_expand_tabs() {
        let spec = syntax::language("a.rs").unwrap();
        let out = lines(&["\tlet x = 1;".into(), "/* a".into(), "b */".into()], spec, colours(Color::BLACK), Color::WHITE);
        let text = |l: &Vec<Span<'_, (), Font>>| l.iter().map(|s| s.text.to_string()).collect::<String>();
        assert_eq!(text(&out[0]), "    let x = 1;");
        assert_eq!(out[2].len(), 1, "the comment opened above colours this whole line");
    }
}
