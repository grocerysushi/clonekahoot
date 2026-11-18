use crate::{
    game::{self, DEFAULT_ROUND, MAX_QUESTIONS, Question, Rooms},
    web::{SAMPLE, create_room, escape, page},
};
use axum::{
    Form,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{collections::HashMap, fmt::Write, time::Duration};

const TIMES: [u64; 6] = [5, 10, 20, 30, 60, 120];

struct Card {
    prompt: String,
    choices: Vec<String>,
    binary: bool,
    correct: Option<usize>,
    seconds: u64,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            choices: vec![String::new(); 4],
            binary: false,
            correct: None,
            seconds: DEFAULT_ROUND.as_secs(),
        }
    }
}

#[derive(Deserialize)]
pub struct EditorQuery {
    #[serde(default)]
    sample: bool,
}

pub async fn open(Query(query): Query<EditorQuery>) -> Response {
    let cards = if query.sample {
        game::parse_quiz(SAMPLE)
            .expect("the built-in sample must be valid")
            .into_iter()
            .map(|question| Card {
                prompt: question.prompt,
                choices: question.choices,
                binary: false,
                correct: Some(question.correct),
                seconds: question.duration.as_secs(),
            })
            .collect()
    } else {
        vec![Card::default()]
    };
    render(&cards, None)
}

pub async fn edit(
    State(rooms): State<Rooms>,
    Form(mut fields): Form<HashMap<String, String>>,
) -> Response {
    let Some(count) = fields
        .remove("count")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| (1..=MAX_QUESTIONS).contains(count))
    else {
        return (
            StatusCode::BAD_REQUEST,
            render(&[Card::default()], Some("Invalid question count.")),
        )
            .into_response();
    };
    let mut cards: Vec<_> = (0..count)
        .map(|index| {
            let mut take = |field: &str| {
                fields
                    .remove(&format!("{field}-{index}"))
                    .unwrap_or_default()
            };
            let binary = take("format") == "binary";
            Card {
                binary,
                prompt: take("prompt"),
                choices: if binary {
                    vec!["True".into(), "False".into()]
                } else {
                    (0..4)
                        .map(|choice| take(&format!("choice-{choice}")))
                        .collect()
                },
                correct: take("correct")
                    .parse::<usize>()
                    .ok()
                    .filter(|choice| *choice < 4),
                seconds: take("time")
                    .parse()
                    .ok()
                    .filter(|time| TIMES.contains(time))
                    .unwrap_or(DEFAULT_ROUND.as_secs()),
            }
        })
        .collect();

    let command = fields.remove("command").unwrap_or_default();
    match command.as_str() {
        "add" | "add-binary" if count < MAX_QUESTIONS => {
            let mut card = Card::default();
            if command == "add-binary" {
                card.binary = true;
                card.choices = vec!["True".into(), "False".into()];
            }
            cards.push(card);
            render(&cards, None)
        }
        "host" | "" => {
            let mut questions = Vec::with_capacity(count);
            for (index, card) in cards.iter().enumerate() {
                let valid_text = |text: &str| (1..=160).contains(&text.trim().chars().count());
                let Some(correct) = card.correct.filter(|choice| {
                    *choice < card.choices.len()
                        && valid_text(&card.prompt)
                        && card.choices.iter().all(|choice| valid_text(choice))
                }) else {
                    let message = format!(
                        "Question {} needs a prompt, all answers, and one correct choice. Enter 1–160 characters per field.",
                        index + 1
                    );
                    return (StatusCode::BAD_REQUEST, render(&cards, Some(&message)))
                        .into_response();
                };
                questions.push(Question {
                    prompt: card.prompt.trim().into(),
                    choices: card
                        .choices
                        .iter()
                        .map(|choice| choice.trim().into())
                        .collect(),
                    correct,
                    duration: Duration::from_secs(card.seconds),
                });
            }
            create_room(&rooms, questions)
        }
        _ => {
            if let Some((action, index)) = command.split_once('-')
                && let Ok(index) = index.parse::<usize>()
                && index < count
            {
                match action {
                    "remove" if count > 1 => {
                        cards.remove(index);
                    }
                    "up" if index > 0 => cards.swap(index, index - 1),
                    "down" if index + 1 < count => cards.swap(index, index + 1),
                    _ => {}
                }
            }
            render(&cards, None)
        }
    }
}

fn render(cards: &[Card], error: Option<&str>) -> Response {
    let count = cards.len();
    let mut body = String::from(
        r#"<div class="editor-heading"><h1>Create a quiz</h1>
<p>Add questions and mark the correct answers.</p></div>"#,
    );
    if let Some(error) = error {
        write!(
            body,
            r#"<p class="notice" role="alert">{}</p>"#,
            escape(error)
        )
        .unwrap();
    }
    write!(
        body,
        r#"<form class="editor" method="post" action="/editor">
<input type="hidden" name="count" value="{count}">
<button name="command" value="host" hidden>Create room</button>"#
    )
    .unwrap();

    body.push_str(
        r#"<nav class="question-nav" aria-label="Quiz questions"><h2>Questions</h2><ol>"#,
    );
    for (index, card) in cards.iter().enumerate() {
        let title = if card.prompt.trim().is_empty() {
            if card.binary {
                "True or false"
            } else {
                "Multiple choice"
            }
        } else {
            card.prompt.trim()
        };
        write!(
            body,
            r##"<li><a href="#question-{index}">{}</a></li>"##,
            escape(title)
        )
        .unwrap();
    }
    body.push_str("</ol>");
    if count < MAX_QUESTIONS {
        body.push_str(
            r#"<details class="format-picker"><summary>Add question</summary>
<div class="format-options">
<button name="command" value="add">Multiple choice</button>
<button name="command" value="add-binary">True or false</button>
</div></details>"#,
        );
    }
    body.push_str(r#"</nav><div class="question-list">"#);
    for (index, card) in cards.iter().enumerate() {
        let number = index + 1;
        let prompt = escape(&card.prompt);
        let (format, kind) = if card.binary {
            ("True or false", "binary")
        } else {
            ("Multiple choice", "multiple")
        };
        write!(
            body,
            r#"<section class="question-card" id="question-{index}" aria-labelledby="heading-{index}">
<input type="hidden" name="format-{index}" value="{kind}">
<div class="question-heading"><div><span class="question-format">{format}</span><h2 id="heading-{index}">Question {number}</h2></div>
<label class="round-time">Time limit <select name="time-{index}">"#
        )
        .unwrap();
        for seconds in TIMES {
            let selected = if seconds == card.seconds {
                " selected"
            } else {
                ""
            };
            write!(
                body,
                r#"<option value="{seconds}"{selected}>{seconds} seconds</option>"#
            )
            .unwrap();
        }
        write!(body, r#"</select></label></div>
<label>Question text<input name="prompt-{index}" value="{prompt}" maxlength="160" placeholder="Type your question"></label>
<fieldset><legend>Answer choices</legend><div class="answer-grid">"#).unwrap();

        for (choice, answer) in card.choices.iter().enumerate() {
            let label = ["A", "B", "C", "D"][choice];
            let value = escape(answer);
            let checked = if card.correct == Some(choice) {
                " checked"
            } else {
                ""
            };
            body.push_str(r#"<div class="answer-card">"#);
            let label = if card.binary {
                write!(body, "<strong>{value}</strong>").unwrap();
                value.as_str()
            } else {
                write!(body, r#"<label>Answer {label}<input name="choice-{choice}-{index}" value="{value}" maxlength="160" placeholder="Type an answer"></label>"#).unwrap();
                label
            };
            write!(body, r#"<label class="correct-choice"><input type="radio" name="correct-{index}" value="{choice}"{checked}> Mark {label} correct</label></div>"#).unwrap();
        }
        body.push_str(r#"</div></fieldset><div class="editor-actions">"#);
        for (show, action, label) in [
            (index > 0, "up", "Move up"),
            (index + 1 < count, "down", "Move down"),
            (count > 1, "remove", "Remove question"),
        ] {
            if show {
                write!(body, r#"<button class="secondary" name="command" value="{action}-{index}" aria-label="{label} {number}">{label}</button>"#).unwrap();
            }
        }
        body.push_str("</div></section>");
    }
    body.push_str(r#"<div class="editor-actions editor-footer">"#);
    body.push_str(r#"<button name="command" value="host">Create room</button></div></div></form>"#);
    page("Create a quiz", &body, None)
}
