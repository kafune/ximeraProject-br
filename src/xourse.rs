//! The xourse file (`\documentclass{xourse}`) lists the activities of a course
//! in pedagogical order.  Ximera builds the activity strip, the breadcrumb and
//! the previous/next links from it, so the reader does the same.

use crate::ximera;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStyle {
    /// Set by `\chapterstyle`: the card opens a chapter.
    Chapter,
    /// Set by `\sectionstyle` or `\activitystyle` (the default).
    Section,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XourseItem {
    Activity {
        path: String,
        style: CardStyle,
    },
    /// `\part{Title}`: a heading card without an activity.
    Part {
        title: String,
    },
}

pub fn parse(source: &str) -> Vec<XourseItem> {
    let mut items = Vec::new();
    let mut style = CardStyle::Section;
    for line in source.lines().map(strip_comment) {
        let mut rest = line;
        while let Some(at) = rest.find('\\') {
            rest = &rest[at + 1..];
            let name_length = rest
                .chars()
                .take_while(|character| character.is_ascii_alphabetic())
                .map(char::len_utf8)
                .sum::<usize>();
            let (name, after) = rest.split_at(name_length);
            rest = after;
            match name {
                "chapterstyle" => style = CardStyle::Chapter,
                "sectionstyle" | "activitystyle" => style = CardStyle::Section,
                "part" => {
                    if let Some((argument, end)) = braced_argument(after) {
                        if let Some(title) = ximera::plain_text(argument) {
                            items.push(XourseItem::Part { title });
                        }
                        rest = &after[end..];
                    }
                }
                "activity" => {
                    if let Some((argument, end)) = braced_argument(after) {
                        items.push(XourseItem::Activity {
                            path: normalize_path(argument),
                            style,
                        });
                        rest = &after[end..];
                    }
                }
                _ => {}
            }
        }
    }
    items
}

/// Paths of the activities, in order.
pub fn activity_paths(items: &[XourseItem]) -> impl Iterator<Item = &str> {
    items.iter().filter_map(|item| match item {
        XourseItem::Activity { path, .. } => Some(path.as_str()),
        XourseItem::Part { .. } => None,
    })
}

fn strip_comment(line: &str) -> &str {
    let mut previous = None;
    for (index, character) in line.char_indices() {
        if character == '%' && previous != Some('\\') {
            return &line[..index];
        }
        previous = Some(character);
    }
    line
}

/// `[options]{argument}` after a command; returns the argument and the offset
/// just past its closing brace.
fn braced_argument(input: &str) -> Option<(&str, usize)> {
    let mut at = input.len() - input.trim_start().len();
    if input[at..].starts_with('[') {
        at = ximera::balanced_end(input, at, '[', ']')?;
        at += input[at..].len() - input[at..].trim_start().len();
    }
    if !input[at..].starts_with('{') {
        return None;
    }
    let end = ximera::balanced_end(input, at, '{', '}')?;
    Some((&input[at + 1..end - 1], end))
}

fn normalize_path(argument: &str) -> String {
    let mut path = argument.trim().replace('\\', "/");
    while let Some(stripped) = path.strip_prefix("./") {
        path = stripped.to_owned();
    }
    if !path.ends_with(".tex") {
        path.push_str(".tex");
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_activities_with_their_styles_and_parts() {
        let items = parse(
            "\\documentclass{xourse}\n\\title{Calculus 1}\n\\begin{document}\n\
             %\\activity{comentada/x.tex}\n\
             \\part{Limites}\n\
             \\chapterstyle\n\\activity{whatIsALimit/titlePage.tex}\n\
             \\sectionstyle\n\\activity{whatIsALimit/breakGround} \\activity{whatIsALimit/digIn.tex} % fim\n\
             \\end{document}",
        );
        assert_eq!(
            items,
            vec![
                XourseItem::Part {
                    title: "Limites".into()
                },
                XourseItem::Activity {
                    path: "whatIsALimit/titlePage.tex".into(),
                    style: CardStyle::Chapter,
                },
                XourseItem::Activity {
                    path: "whatIsALimit/breakGround.tex".into(),
                    style: CardStyle::Section,
                },
                XourseItem::Activity {
                    path: "whatIsALimit/digIn.tex".into(),
                    style: CardStyle::Section,
                },
            ]
        );
        assert_eq!(activity_paths(&items).count(), 3);
    }
}
