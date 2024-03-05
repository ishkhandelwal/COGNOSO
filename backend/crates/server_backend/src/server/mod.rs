pub mod database;
pub mod llm;
pub mod search_engine;
pub mod utils;

use crate::api_structs;
use crate::AndyError;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::Buf;
use hyper::body::Bytes;
use hyper::header::HeaderValue;
use hyper::{Request, Response};

pub struct SharedState {
    pub database: database::Database,
    pub llm_runner: llm::LlmRunner,
    pub search_engine: tokio::sync::Mutex<search_engine::SearchEngine>,
}

pub async fn main_service(
    req: Request<hyper::body::Incoming>,
    state: std::sync::Arc<SharedState>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    match handle_request(req, state).await {
        Ok(x) => Ok(x),
        Err(e) => {
            println!("got error: {:?}", e);
            let mut err_response = Response::new(Full::new(Bytes::from(format!("{:?}", e))));
            *err_response.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
            err_response.headers_mut().insert(
                "content-type",
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            err_response
                .headers_mut()
                .insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
            Ok(err_response)
        }
    }
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    state: std::sync::Arc<SharedState>,
) -> Result<Response<Full<Bytes>>, AndyError> {
    let uri = req.uri().path();
    let method = req.method();
    println!("got request method = {}, endpoint = {}", method, uri);
    macro_rules! endpoints {
        ($(($meth:pat, $uri:pat, $func:expr)),*) => {
            match (method, uri) {
                $((&$meth, $uri) => {
                    let bytes = req.collect().await?.to_bytes();
                    let thing = serde_json::from_reader(bytes.reader())?;
                    let body_struct = $func(thing, state).await?;
                    let body_str = serde_json::to_string(&body_struct)?;
                    utils::make_response(
                        hyper::StatusCode::OK,
                        vec![(hyper::header::CONTENT_TYPE, "application/json"),
                            (hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
                        body_str
                    )
                },)*
                (&hyper::Method::OPTIONS, _) => {
                    //TODO this assumes every endpoint is a POST request in CORS headers
                    utils::cors_preflight_headers(req, vec!("POST"))
                },
                (method, endpoint) => {
                    println!("404 REQUEST: endpoint = {}, method = {}", endpoint, method);
                    utils::make_response(
                        hyper::StatusCode::NOT_FOUND,
                        vec![(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8"),
                        (hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
                        "NOT FOUND".to_owned()
                    )
                }
            }
        }
    }

    endpoints!(
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_CREATE_CARD_DECK,
            create_card_deck
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_DELETE_CARD_DECK,
            delete_card_deck
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_CREATE_CARD,
            create_card
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_DELETE_CARD,
            delete_card
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_NEW_USER,
            new_user
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_LIST_CARD_DECKS,
            list_card_decks
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_LIST_CARDS,
            list_cards
        ),
        (hyper::Method::POST, api_structs::ENDPOINT_LOGIN, login),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_CREATE_DECK_PDF,
            create_deck_pdf
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_DELETE_USER,
            delete_user
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_CHANGE_PASSWORD,
            change_password
        ),
        (hyper::Method::POST, api_structs::ENDPOINT_AI_TEST, ai_test),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_GET_DECK,
            get_deck
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_SEARCH_DECKS,
            search
        ),
        (
            hyper::Method::POST,
            api_structs::ENDPOINT_EDIT_CARD,
            edit_card
        )
    )
}

async fn login(
    info: api_structs::LoginRequest,
    state: std::sync::Arc<SharedState>,
) -> Result<api_structs::LoginResponse, AndyError> {
    let user_id = state.database.get_user_id(&info.email);
    let access_token = state.database.new_session(user_id, info.password)?;
    Ok(api_structs::LoginResponse {
        access_token,
        user_id,
    })
}

async fn create_card_deck(
    info: api_structs::CreateCardDeck,
    state: std::sync::Arc<SharedState>,
) -> Result<(), AndyError> {
    let user_id = state.database.validate_token(info.access_token)?;
    state.database.new_card_deck(user_id, info.deck_name)?;
    Ok(())
}

async fn get_deck(
    info: api_structs::GetDeckRequest,
    state: std::sync::Arc<SharedState>,
) -> Result<api_structs::GetDeckResponse, AndyError> {
    let name = state.database.get_deck_info(info.user_id, info.deck_id)?;
    Ok(name)
}

async fn delete_card_deck(
    info: api_structs::DeleteCardDeck,
    state: std::sync::Arc<SharedState>,
) -> Result<(), AndyError> {
    let user_id = state.database.validate_token(info.access_token)?;
    state.database.delete_card_deck(user_id, info.deck_id)?;
    Ok(())
}

async fn create_card(
    info: api_structs::CreateCard,
    state: std::sync::Arc<SharedState>,
) -> Result<(), AndyError> {
    let user_id = state.database.validate_token(info.access_token)?;
    state
        .database
        .new_card(user_id, info.deck_id, info.question, info.answer)?;
    Ok(())
}

async fn delete_card(
    info: api_structs::DeleteCard,
    state: std::sync::Arc<SharedState>,
) -> Result<(), AndyError> {
    let user_id = state.database.validate_token(info.access_token)?;
    state
        .database
        .delete_card(user_id, info.deck_id, info.card_index)?;
    Ok(())
}

async fn edit_card(
    info: api_structs::EditCard,
    state: std::sync::Arc<SharedState>,
) -> Result<(), AndyError> {
    let user_id = state.database.validate_token(info.access_token)?;
    state.database.edit_card(
        user_id,
        info.deck_id,
        info.card_index,
        info.new_question,
        info.new_answer,
    )?;
    Ok(())
}

async fn new_user(
    info: api_structs::NewUser,
    state: std::sync::Arc<SharedState>,
) -> Result<(), AndyError> {
    state
        .database
        .new_user(info.user_name, info.email, info.password)?;
    Ok(())
}

async fn delete_user(
    info: api_structs::DeleteUser,
    state: std::sync::Arc<SharedState>,
) -> Result<(), AndyError> {
    state.database.delete_user(info.email, info.password)?;
    Ok(())
}

async fn change_password(
    info: api_structs::ChangePassword,
    state: std::sync::Arc<SharedState>,
) -> Result<(), AndyError> {
    state
        .database
        .change_password(info.email, info.old_password, info.new_password)?;
    Ok(())
}

async fn list_card_decks(
    info: api_structs::ListCardDecks,
    state: std::sync::Arc<SharedState>,
) -> Result<api_structs::ListCardDecksResponse, AndyError> {
    let user_id = state.database.validate_token(info.access_token)?;
    state.database.list_card_decks(user_id)
}

async fn list_cards(
    info: api_structs::ListCards,
    state: std::sync::Arc<SharedState>,
) -> Result<api_structs::ListCardsResponse, AndyError> {
    state.database.list_cards(info.user_id, info.deck_id)
}

async fn search(
    info: api_structs::SearchDecksRequest,
    state: std::sync::Arc<SharedState>,
) -> Result<api_structs::SearchDecksResponse, AndyError> {
    let thing = state
        .search_engine
        .lock()
        .await
        .search_prompt(&info.prompt, 5)
        .await?;

    Ok(api_structs::SearchDecksResponse { decks: thing })
}

async fn create_deck_pdf(
    info: api_structs::UploadPdf,
    state: std::sync::Arc<SharedState>,
) -> Result<(), AndyError> {
    let _user_id = state.database.validate_token(info.access_token)?;

    let url = data_url::DataUrl::process(&info.file_bytes_base64).unwrap();
    let (body, _fragment) = url.decode_to_vec().unwrap();

    let _lines = pdf_parser::extract_text(&body)?;

    todo!()
}

async fn ai_test(
    info: api_structs::AiPromptTest,
    state: std::sync::Arc<SharedState>,
) -> Result<String, AndyError> {
    let ai_response = state.llm_runner.submit_prompt(info.prompt).await?;

    Ok(ai_response)
}
