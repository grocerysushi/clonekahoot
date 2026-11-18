use crate::game::{self, Phase, Player, Room, Rooms};
use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use std::{fmt::Write, time::Instant};
use uuid::Uuid;

pub(crate) const SAMPLE: &str = "Which planet has the shortest year? | Mars | Venus | Mercury | Earth | 3\nWhat do bees collect to make honey? | Nectar | Seeds | Sap | Dew | 1\nWhich of these is a prime number? | 21 | 27 | 39 | 41 | 4";

pub(crate) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn page(title: &str, body: &str, refresh: Option<u64>) -> Response {
    let refresh = refresh
        .map(|seconds| format!(r#"<meta http-equiv="refresh" content="{seconds}">"#))
        .unwrap_or_default();
    let html = Html(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">{refresh}
<title>{title} · Rally Quiz</title><link rel="stylesheet" href="/style.css"></head>
<body><header><a href="/">Rally<span>Quiz</span></a></header><main>{body}</main></body></html>"#
    ));
    ([(header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer"),
      (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'")],
     html).into_response()
}

pub async fn stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("style.css"),
    )
}

fn landing(error: &str) -> Response {
    let notice = if error.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="notice" role="alert">{}</p>"#, escape(error))
    };
    page(
        "Play",
        &format!(
            r#"<section class="intro"><h1>Bring your own trivia.</h1>
<p>Start a live quiz, share the room code, and see who tops the standings.</p></section>
{notice}<div class="grid"><section><h2>Join a game</h2><form action="/join" method="post">
<label>Room code<input name="code" required maxlength="6" pattern="[A-Fa-f0-9]{{6}}" autocomplete="off"></label>
<label>Nickname<input name="name" required maxlength="24" autocomplete="nickname"></label>
<button>Join room</button></form></section><section><h2>Host a game</h2>
<p>Write your own questions or start with a sample quiz.</p>
<div class="editor-actions"><a class="button" href="/editor">Create quiz</a>
<a href="/editor?sample=true">Open sample quiz</a></div></section></div>"#
        ),
        None,
    )
}

pub async fn home() -> Response {
    landing("")
}

#[derive(Deserialize)]
pub struct QuizForm {
    quiz: String,
}

#[derive(Deserialize)]
pub struct JoinForm {
    code: String,
    name: String,
}

#[derive(Deserialize)]
pub struct AdvanceForm {
    phase: String,
    token: Uuid,
}

#[derive(Deserialize)]
pub struct AnswerForm {
    round: usize,
    choice: usize,
    token: Uuid,
}

fn session(headers: &HeaderMap, code: &str) -> Option<Uuid> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    let key = format!("rally_{code}");
    cookies
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| value.parse().ok())
}

fn enter(code: &str, id: Uuid) -> Response {
    let location = format!("/room/{code}");
    let cookie =
        format!("rally_{code}={id}; Path={location}; HttpOnly; SameSite=Strict; Max-Age=28800");
    ([(header::SET_COOKIE, cookie)], Redirect::to(&location)).into_response()
}

fn failure(status: StatusCode, message: &str) -> Response {
    (
        status,
        page(
            "Room",
            &format!(r#"<section><h1>{message}</h1><p><a href="/">Return home</a></p></section>"#),
            None,
        ),
    )
        .into_response()
}

pub async fn create(State(rooms): State<Rooms>, Form(form): Form<QuizForm>) -> Response {
    let questions = match game::parse_quiz(&form.quiz) {
        Ok(questions) => questions,
        Err(error) => return (StatusCode::BAD_REQUEST, landing(error)).into_response(),
    };
    create_room(&rooms, questions)
}

pub(crate) fn create_room(rooms: &Rooms, questions: Vec<game::Question>) -> Response {
    let mut rooms = rooms.lock().unwrap();
    rooms.retain(|_, room| room.created.elapsed() < game::ROOM_LIFETIME);
    if rooms.len() >= 128 {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "All room slots are in use.",
        );
    }
    let code = loop {
        let id = Uuid::new_v4().simple().to_string();
        let code = id[..6].to_ascii_uppercase();
        if !rooms.contains_key(&code) {
            break code;
        }
    };
    let room = Room::new(questions);
    let host = room.host;
    rooms.insert(code.clone(), room);
    enter(&code, host)
}

pub async fn join(State(rooms): State<Rooms>, Form(form): Form<JoinForm>) -> Response {
    let code = form.code.trim().to_ascii_uppercase();
    let name = form.name.trim();
    if name.is_empty() || name.chars().count() > 24 || name.chars().any(char::is_control) {
        return (
            StatusCode::BAD_REQUEST,
            landing("Enter a nickname of 1–24 characters."),
        )
            .into_response();
    }
    let mut rooms = rooms.lock().unwrap();
    let Some(room) = rooms
        .get_mut(&code)
        .filter(|room| room.created.elapsed() < game::ROOM_LIFETIME)
    else {
        return (
            StatusCode::NOT_FOUND,
            landing("That room code was not found."),
        )
            .into_response();
    };
    if !matches!(room.phase, Phase::Lobby) || room.players.len() >= game::MAX_PLAYERS {
        return (
            StatusCode::CONFLICT,
            landing("That room is full or the game has started."),
        )
            .into_response();
    }
    if room
        .players
        .values()
        .any(|player| player.name.to_lowercase() == name.to_lowercase())
    {
        return (
            StatusCode::CONFLICT,
            landing("That nickname is already in use in this room."),
        )
            .into_response();
    }
    let id = Uuid::new_v4();
    room.players.insert(
        id,
        Player {
            name: name.into(),
            score: 0,
            answer: None,
        },
    );
    enter(&code, id)
}

pub async fn room(
    State(rooms): State<Rooms>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Response {
    let mut rooms = rooms.lock().unwrap();
    let Some(room) = rooms
        .get_mut(&code)
        .filter(|room| room.created.elapsed() < game::ROOM_LIFETIME)
    else {
        return failure(StatusCode::NOT_FOUND, "Room not found.");
    };
    let Some(id) = session(&headers, &code) else {
        return failure(StatusCode::FORBIDDEN, "Join with the room code first.");
    };
    let host = id == room.host;
    let player = room.players.get(&id);
    if !host && player.is_none() {
        return failure(StatusCode::FORBIDDEN, "Join with the room code first.");
    }

    let now = Instant::now();
    room.tick(now);
    let player = room.players.get(&id);
    let label = player
        .map(|player| escape(&player.name))
        .unwrap_or_else(|| "Host".into());
    let mut body = format!(
        r#"<div class="roomline"><span>{label}</span><span>Room <strong>{code}</strong></span></div>"#
    );
    let mut refresh = Some(2);

    match room.phase {
        Phase::Lobby => {
            write!(body, "<section><h1>Waiting room</h1><p>{} joined. Share the code <strong>{code}</strong> to invite players.</p>", room.players.len()).unwrap();
            if host && !room.players.is_empty() {
                control(&mut body, &code, id, "lobby", "Start game");
            }
            if !room.players.is_empty() {
                standings(&mut body, room);
            }
        }
        Phase::Question { index, deadline } => {
            let question = &room.questions[index];
            let seconds = deadline.duration_since(now).as_millis().div_ceil(1000);
            write!(
                body,
                r#"<section><p class="round">Question {} of {} · {seconds}s left</p><h1>{}</h1>"#,
                index + 1,
                room.questions.len(),
                escape(&question.prompt)
            )
            .unwrap();
            body.push_str(r#"<div class="choices">"#);
            for (choice, value) in question.choices.iter().enumerate() {
                let value = escape(value);
                match player {
                    Some(player) if player.answer.is_none() => {
                        write!(
                            body,
                            r#"<form action="/room/{code}/answer" method="post">
<input type="hidden" name="token" value="{id}"><input type="hidden" name="round" value="{index}">
<button name="choice" value="{choice}">{value}</button></form>"#
                        )
                        .unwrap();
                    }
                    _ => write!(body, r#"<div class="choice">{value}</div>"#).unwrap(),
                }
            }
            body.push_str("</div>");
            if host {
                let answered = room
                    .players
                    .values()
                    .filter(|player| player.answer.is_some())
                    .count();
                write!(
                    body,
                    "<p>{answered} of {} answered.</p>",
                    room.players.len()
                )
                .unwrap();
                control(
                    &mut body,
                    &code,
                    id,
                    &format!("question-{index}"),
                    "Reveal answer",
                );
            } else if player.is_some_and(|player| player.answer.is_some()) {
                body.push_str("<p>Answer recorded. Waiting for the reveal.</p>");
            }
        }
        Phase::Results { index } => {
            let question = &room.questions[index];
            write!(
                body,
                "<section><h1>Round results</h1><p>Correct answer <strong>{}</strong></p>",
                escape(&question.choices[question.correct])
            )
            .unwrap();
            if let Some(player) = player {
                let outcome = match player.answer {
                    Some(choice) if choice == question.correct => "You got it right.",
                    Some(_) => "You missed this one.",
                    None => "Time ran out before you answered.",
                };
                write!(body, "<p>{outcome} Your score is {}.</p>", player.score).unwrap();
            }
            standings(&mut body, room);
            if host {
                refresh = None;
                let label = if index + 1 == room.questions.len() {
                    "Finish game"
                } else {
                    "Next question"
                };
                control(&mut body, &code, id, &format!("results-{index}"), label);
            }
        }
        Phase::Finished => {
            refresh = None;
            body.push_str("<section><h1>Final standings</h1>");
            standings(&mut body, room);
            body.push_str(r#"<p><a href="/">Play again</a></p>"#);
        }
    }
    body.push_str("</section>");
    page("Game room", &body, refresh)
}

fn control(body: &mut String, code: &str, token: Uuid, phase: &str, label: &str) {
    write!(
        body,
        r#"<form action="/room/{code}/advance" method="post">
<input type="hidden" name="token" value="{token}"><input type="hidden" name="phase" value="{phase}">
<button>{label}</button></form>"#
    )
    .unwrap();
}

fn standings(body: &mut String, room: &Room) {
    let mut players: Vec<_> = room.players.values().collect();
    players.sort_unstable_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    body.push_str(r#"<table><thead><tr><th scope="col">Player</th><th scope="col">Points</th></tr></thead><tbody>"#);
    for player in players {
        write!(
            body,
            "<tr><td>{}</td><td>{}</td></tr>",
            escape(&player.name),
            player.score
        )
        .unwrap();
    }
    body.push_str("</tbody></table>");
}

pub async fn advance(
    State(rooms): State<Rooms>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<AdvanceForm>,
) -> Response {
    let mut rooms = rooms.lock().unwrap();
    let Some(room) = rooms
        .get_mut(&code)
        .filter(|room| room.created.elapsed() < game::ROOM_LIFETIME)
    else {
        return failure(StatusCode::NOT_FOUND, "Room not found.");
    };
    if session(&headers, &code) != Some(room.host) || form.token != room.host {
        return failure(StatusCode::FORBIDDEN, "Only the host can advance the game.");
    }
    room.advance(&form.phase, Instant::now());
    Redirect::to(&format!("/room/{code}")).into_response()
}

pub async fn answer(
    State(rooms): State<Rooms>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<AnswerForm>,
) -> Response {
    let mut rooms = rooms.lock().unwrap();
    let Some(room) = rooms
        .get_mut(&code)
        .filter(|room| room.created.elapsed() < game::ROOM_LIFETIME)
    else {
        return failure(StatusCode::NOT_FOUND, "Room not found.");
    };
    if session(&headers, &code) != Some(form.token) || !room.players.contains_key(&form.token) {
        return failure(StatusCode::FORBIDDEN, "Only joined players can answer.");
    }
    room.answer(form.token, form.round, form.choice, Instant::now());
    Redirect::to(&format!("/room/{code}")).into_response()
}
