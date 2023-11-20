#[macro_use]
extern crate serde;

use ic_cdk::caller;
use validator::Validate;
use candid::{Decode, Encode};
use ic_cdk::api::time;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::{borrow::Cow, cell::RefCell};
use std::collections::HashMap;

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct Quiz {
    id: u64,
    author: String,
    question: String,
    options: Vec<String>,
    answers: HashMap<String, u32>,
    created_at: u64,
    updated_at: Option<u64>,
}

// a trait that must be implemented for a struct that is stored in a stable struct
impl Storable for Quiz {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// another trait that must be implemented for a struct that is stored in a stable struct
impl BoundedStorable for Quiz {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
        static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
            MemoryManager::init(DefaultMemoryImpl::default())
        );

        static ID_COUNTER: RefCell<IdCell> = RefCell::new(
            IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
                .expect("Cannot create a counter")
        );

        static STORAGE: RefCell<StableBTreeMap<u64, Quiz, Memory>> =
            RefCell::new(StableBTreeMap::init(
                MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
        ));
    }

#[derive(candid::CandidType, Serialize, Deserialize, Default, Validate)]
struct QuizPayload {
    #[validate(length(min = 10))]
    question: String,
    #[validate(length(min = 2))]
    options: Vec<String>,
}


#[ic_cdk::query]
fn get_all_quiz() -> Result<Vec<Quiz>, Error> {
    let quizzes_map : Vec<(u64, Quiz)> =  STORAGE.with(|service| service.borrow().iter().collect());
    let length = quizzes_map.len();
    let mut quizzes: Vec<Quiz> = Vec::new();
    for key in 0..length {
        quizzes.push(quizzes_map.get(key).unwrap().clone().1);
    }

    if quizzes.len() > 0 {
        Ok(quizzes)
    }else {
        Err(Error::NotFound {
            msg: format!("There are currently no quiz"),
        })
    }
}


#[ic_cdk::query]
fn get_quiz(id: u64) -> Result<Quiz, Error> {
    match _get_quiz(&id) {
        Some(message) => Ok(message),
        None => Err(Error::NotFound {
            msg: format!("a quiz with id={} not found", id),
        }),
    }
}

fn _get_quiz(id: &u64) -> Option<Quiz> {
    STORAGE.with(|s| s.borrow().get(id))
}


#[ic_cdk::update]
fn create_quiz(payload: QuizPayload) -> Option<Quiz> {
    payload.validate().expect("Input validation failed");
    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("cannot increment id counter");

    let mut answers = HashMap::new();

    for option in &payload.options {
        answers.insert(String::from(option), 0);
    }


    let quiz = Quiz {
        id,
        author: caller().to_string(),
        question: payload.question,
        options: payload.options,
        answers,
        created_at: time(),
        updated_at: None,
    };
    do_insert(&quiz);
    Some(quiz)
}


// helper method to perform insert.
fn do_insert(quiz: &Quiz) {
    STORAGE.with(|service| service.borrow_mut().insert(quiz.id, quiz.clone()));
}


#[ic_cdk::update]
fn update_quiz(id: u64, payload: QuizPayload) -> Result<Quiz, Error> {
    payload.validate().expect("Input validation failed");
    let quiz_option: Option<Quiz> = STORAGE.with(|service| service.borrow().get(&id));

    match quiz_option {
        Some(mut quiz) => {
            assert!(quiz.author == caller().to_string(), "Not author of quiz");

            let mut answers = HashMap::new();

            for option in &payload.options {
                answers.insert(String::from(option), 0);
            }

            quiz.question = payload.question;
            quiz.options = payload.options;
            quiz.answers = answers;
            quiz.updated_at = Some(time());
            do_insert(&quiz);
            Ok(quiz)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't update a quiz with id={}. quiz not found",
                id
            ),
        }),
    }
}


#[ic_cdk::update]
fn delete_quiz(id: u64) -> Result<Quiz, Error> {
    let quiz: Option<Quiz> = STORAGE.with(|service| service.borrow().get(&id));
    assert!(quiz.is_some(), "Quiz doesn't exist");
    assert!(quiz.unwrap().author == caller().to_string(), "Not author of quiz");
    match STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(quiz) => Ok(quiz),
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't delete a quiz with id={}. quiz not found.",
                id
            ),
        }),
    }
}


#[ic_cdk::update]
fn answer_quiz(id: u64, option: String) -> Result<Quiz, Error> {

    let quiz_option: Option<Quiz> = STORAGE.with(|service| service.borrow().get(&id));

    match quiz_option {

        Some(mut quiz) => {

            // Check if the selected option is valid
            if quiz.options.contains(&option) {
                if let Some(answer_count) = quiz.answers.get_mut(&option) {
                    *answer_count += 1;
                }
                quiz.updated_at = Some(time());
                do_insert(&quiz);
                Ok(quiz)
            } else {
                // Return an error if the selected option is not valid
                Err(Error::NotFound {
                    msg: format!("The option '{}' is not found for this quiz.", option),
                })
            }
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't cast a quiz with id={}. quiz not found",
                id
            ),
        }),
    }
}

#[derive(candid::CandidType, Deserialize, Serialize)]
enum Error {
    NotFound { msg: String },
}

// need this to generate candid
ic_cdk::export_candid!();
