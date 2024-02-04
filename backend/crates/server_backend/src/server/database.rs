use crate::AndyError;
use redb::ReadableTable;
use sha2::Digest;
use std::hash::Hasher;

const SHA265_NUM_BYTES: usize = 100;

#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct UserEntry {
    username: String,
    user_id: u64,
    email: String,
    #[serde_as(as = "serde_with::Bytes")]
    //idk prolly serde will fix this const generics in the future
    password_hash: [u8; SHA265_NUM_BYTES],
    signup_time: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct CardDeck {
    creation_time: u64,
    cards: Vec<Card>,
    name: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct Card {
    question: String,
    answer: String,
}

pub struct Database {
    db: redb::Database,
}

impl Database {
    const USERS_TABLE: redb::TableDefinition<'static, &'static str, UserEntry> =
        redb::TableDefinition::new("users");
    const DECKS_TABLE: redb::TableDefinition<'static, (u64, u64), CardDeck> =
        redb::TableDefinition::new("decks");

    pub fn validate_token(&self, _token: String) -> Result<u64, AndyError> {
        todo!()
    }

    pub fn new(db_path: std::path::PathBuf) -> Result<Self, AndyError> {
        let db = redb::Database::create(db_path)?;
        {
            //create tables
            let write_txn = db.begin_write()?;
            write_txn.open_table(Self::USERS_TABLE)?;
            write_txn.open_table(Self::DECKS_TABLE)?;
            write_txn.commit()?;
        }
        Ok(Self { db })
    }

    pub fn new_user(
        &self,
        user_name: String,
        email: String,
        password: String,
    ) -> Result<(), AndyError> {
        let user_id = hash(&user_name); //todo idk
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(Self::USERS_TABLE)?;
            table.insert(
                user_name.as_str(),
                UserEntry {
                    username: user_name.clone(),
                    user_id,
                    email,
                    password_hash: sha256_hash(password.as_bytes()),
                    signup_time: get_current_unix_time_seconds(),
                },
            )?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn new_card_deck(&self, user_id: u64, deck_name: String) -> Result<(), AndyError> {
        let deck_id = hash(&deck_name);
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(Self::DECKS_TABLE)?;
            table.insert(
                (user_id, deck_id),
                CardDeck {
                    creation_time: get_current_unix_time_seconds(),
                    cards: vec![],
                    name: deck_name,
                },
            )?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn new_card(
        &self,
        user_id: u64,
        deck_id: u64,
        question: String,
        answer: String,
    ) -> Result<(), AndyError> {
        let key = (user_id, deck_id);

        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::DECKS_TABLE)?;
        let mut deck = table.get(key)?.unwrap().value();
        deck.cards.push(Card { question, answer });

        self.insert(key, deck, Self::DECKS_TABLE)?;
        Ok(())
    }

    pub fn list_card_decks(
        &self,
        user_id: u64,
    ) -> Result<api_structs::ListCardDecksResponse, AndyError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::DECKS_TABLE)?;

        let mut deck_ids: Vec<api_structs::CardDeck> = vec![];

        for entry in table.iter()? {
            let entry = entry?;
            let id_pair = entry.0.value();
            if id_pair.0 == user_id {
                let deck = entry.1.value();
                deck_ids.push(api_structs::CardDeck {
                    deck_id: id_pair.1,
                    name: deck.name,
                    num_cards: deck.cards.len().try_into()?,
                });
            }
        }

        Ok(api_structs::ListCardDecksResponse { decks: deck_ids })
    }

    pub fn list_cards(
        &self,
        user_id: u64,
        deck_id: u64,
    ) -> Result<api_structs::ListCardsResponse, AndyError> {
        let key = (user_id, deck_id);

        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::DECKS_TABLE)?;
        let deck = table.get(key)?.unwrap().value();

        Ok(api_structs::ListCardsResponse {
            cards: deck
                .cards
                .into_iter()
                .map(|card: Card| api_structs::Card {
                    question: card.question,
                    answer: card.answer,
                })
                .collect(),
        })
    }

    fn insert<'a, K, V>(
        &self,
        key: K,
        val: V,
        table: redb::TableDefinition<'static, K, V>,
    ) -> Result<(), AndyError>
    where
        K: redb::RedbKey + core::borrow::Borrow<<K as redb::RedbValue>::SelfType<'a>>,
        V: redb::RedbValue + core::borrow::Borrow<<V as redb::RedbValue>::SelfType<'a>>,
    {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(table)?;
            table.insert(key, val)?;
        }
        write_txn.commit()?;

        Ok(())
    }
}

fn sha256_hash(bytes: &[u8]) -> [u8; SHA265_NUM_BYTES] {
    let mut hasher = sha2::Sha256::new();

    hasher.update(bytes);

    let result: Vec<u8> = hasher.finalize().to_vec();

    return result.try_into().unwrap();
}

fn hash<K>(username: K) -> u64
where
    K: std::hash::Hash,
{
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    username.hash(&mut hasher);
    hasher.finish()
}

fn get_current_unix_time_seconds() -> u64 {
    let start = std::time::SystemTime::now();
    start
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

macro_rules! implement_redb_value {
    ($typename:ty, $unique_identifier:expr) => {
        impl redb::RedbValue for $typename {
            type SelfType<'a> = Self;
            type AsBytes<'a> = Vec<u8>;

            fn fixed_width() -> Option<usize> {
                None
            }

            fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
            where
                Self: 'a,
            {
                serde_json::from_slice(data).expect("database deserialization messed up")
            }

            fn as_bytes<'a, 'b: 'a>(value: &Self) -> Vec<u8> {
                serde_json::to_vec(value).expect("database serialization messed up")
            }

            fn type_name() -> redb::TypeName {
                redb::TypeName::new($unique_identifier)
            }
        }
    };
}

implement_redb_value!(CardDeck, "andy_card_deck");
implement_redb_value!(UserEntry, "andy_user_entry");
