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
    choices: [String; 4],
    correct: Option<usize>,
    seconds: u64,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            choices: Default::default(),
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
            Card {
                prompt: take("prompt"),
                choices: std::array::from_fn(|choice| take(&format!("choice-{choice}"))),
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
        "add" if count < MAX_QUESTIONS => {
            cards.push(Card::default());
            render(&cards, None)
        }
        "host" | "" => {
            let mut questions = Vec::with_capacity(count);
            for (index, card) in cards.iter().enumerate() {
                let valid_text = |text: &str| (1..=160).contains(&text.trim().chars().count());
                let Some(correct) = card.correct.filter(|_| {
                    valid_text(&card.prompt) && card.choices.iter().all(|choice| valid_text(choice))
                }) else {
                    let message = format!(
                        "Question {} needs a prompt, four answers, and one correct choice. Enter 1–160 characters per field.",
                        index + 1
                    );
                    return (StatusCode::BAD_REQUEST, render(&cards, Some(&message)))
                        .into_response();
                };
                questions.push(Question {
                    prompt: card.prompt.trim().into(),
                    choices: card.choices.each_ref().map(|choice| choice.trim().into()),
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
<p>Write four answer choices and mark the correct one for each question.</p></div>"#,
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

    for (index, card) in cards.iter().enumerate() {
        let number = index + 1;
        let prompt = escape(&card.prompt);
        write!(
            body,
            r#"<section class="question-card" aria-labelledby="question-{index}">
<div class="question-heading"><h2 id="question-{index}">Question {number}</h2>
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
            write!(body, r#"<div class="answer-card">
<label>Answer {label}<input name="choice-{choice}-{index}" value="{value}" maxlength="160" placeholder="Type an answer"></label>
<label class="correct-choice"><input type="radio" name="correct-{index}" value="{choice}"{checked}> Correct answer {label}</label>
</div>"#).unwrap();
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
    if count < MAX_QUESTIONS {
        body.push_str(
            r#"<button class="secondary" name="command" value="add">Add question</button>"#,
        );
    }
    body.push_str(r#"<button name="command" value="host">Create room</button></div></form>"#);
    page("Create a quiz", &body, None)
}
