use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub type Rooms = Arc<Mutex<HashMap<String, Room>>>;
pub const ROUND: Duration = Duration::from_secs(20);
pub const ROOM_LIFETIME: Duration = Duration::from_secs(8 * 60 * 60);
pub const MAX_PLAYERS: usize = 100;

pub struct Question {
    pub prompt: String,
    pub choices: [String; 4],
    pub correct: usize,
}

pub fn parse_quiz(source: &str) -> Result<Vec<Question>, &'static str> {
    let mut questions = Vec::new();
    for line in source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let fields: Vec<_> = line.split('|').map(str::trim).collect();
        let [prompt, a, b, c, d, correct] = fields.as_slice() else {
            return Err("Each question needs a prompt, four answers, and a correct answer number.");
        };
        if fields
            .iter()
            .any(|field| field.is_empty() || field.chars().count() > 160)
        {
            return Err("Keep each field between 1 and 160 characters.");
        }
        let correct = correct
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=4).contains(n))
            .ok_or("Use a number from 1 to 4 for the correct answer.")?;
        questions.push(Question {
            prompt: (*prompt).into(),
            choices: [(*a).into(), (*b).into(), (*c).into(), (*d).into()],
            correct: correct - 1,
        });
    }
    if !(1..=30).contains(&questions.len()) {
        return Err("Enter between 1 and 30 questions.");
    }
    Ok(questions)
}

#[derive(Clone, Copy)]
pub enum Phase {
    Lobby,
    Question { index: usize, deadline: Instant },
    Results { index: usize },
    Finished,
}

pub struct Player {
    pub name: String,
    pub score: u32,
    pub answer: Option<usize>,
}

pub struct Room {
    pub host: Uuid,
    pub created: Instant,
    pub questions: Vec<Question>,
    pub phase: Phase,
    pub players: HashMap<Uuid, Player>,
}

impl Room {
    pub fn new(questions: Vec<Question>) -> Self {
        Self {
            host: Uuid::new_v4(),
            created: Instant::now(),
            questions,
            phase: Phase::Lobby,
            players: HashMap::new(),
        }
    }

    fn start(&mut self, index: usize, now: Instant) {
        for player in self.players.values_mut() {
            player.answer = None;
        }
        self.phase = Phase::Question {
            index,
            deadline: now + ROUND,
        };
    }

    pub fn tick(&mut self, now: Instant) {
        if let Phase::Question { index, deadline } = self.phase
            && now >= deadline
        {
            self.phase = Phase::Results { index };
        }
    }

    pub fn advance(&mut self, expected: &str, now: Instant) {
        self.tick(now);
        match self.phase {
            Phase::Lobby if expected == "lobby" && !self.players.is_empty() => self.start(0, now),
            Phase::Question { index, .. } if expected == format!("question-{index}") => {
                self.phase = Phase::Results { index };
            }
            Phase::Results { index } if expected == format!("results-{index}") => {
                if index + 1 < self.questions.len() {
                    self.start(index + 1, now);
                } else {
                    self.phase = Phase::Finished;
                }
            }
            _ => {}
        }
    }

    pub fn answer(&mut self, id: Uuid, round: usize, choice: usize, now: Instant) -> bool {
        self.tick(now);
        let Phase::Question { index, deadline } = self.phase else {
            return false;
        };
        if round != index || choice >= 4 {
            return false;
        }
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        if player.answer.is_some() {
            return false;
        }
        player.answer = Some(choice);
        if self.questions[index].correct == choice {
            let remaining = deadline.duration_since(now).as_millis();
            player.score += 500 + (500 * remaining / ROUND.as_millis()) as u32;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_cannot_be_replayed_or_submitted_after_deadline() {
        let mut room = Room::new(
            parse_quiz("First? | A | B | C | D | 2\nSecond? | A | B | C | D | 1").unwrap(),
        );
        let id = Uuid::new_v4();
        room.players.insert(
            id,
            Player {
                name: "Ada".into(),
                score: 0,
                answer: None,
            },
        );
        let now = Instant::now();

        room.advance("lobby", now);
        assert!(room.answer(id, 0, 1, now));
        assert!(!room.answer(id, 0, 1, now));
        assert_eq!(room.players[&id].score, 1000);

        room.advance("question-0", now + ROUND);
        assert!(matches!(room.phase, Phase::Results { index: 0 }));
        room.advance("results-0", now + ROUND);
        assert!(!room.answer(id, 0, 0, now + ROUND));
        assert!(room.players[&id].answer.is_none());
        assert!(!room.answer(id, 1, 0, now + 2 * ROUND));
        assert!(matches!(room.phase, Phase::Results { index: 1 }));
        assert_eq!(room.players[&id].score, 1000);
    }
}
