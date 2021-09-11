use async_trait::async_trait;
use log::debug;
use serde::Deserialize;
use serde_json;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::sync::RwLock;

const SLEEPER_API_URL: &str = "https://api.sleeper.app/v1";
const PLAYERS_DATA_PATH: &str = "data/sleeper_players.json";
const SECS_PER_DAY: u64 = 60 * 60 * 24;

#[derive(Deserialize, Debug)]
struct SleeperTeamMetadata {
    team_name: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct SleeperNflStateApiResponse {
    week: u32,
    season_type: String,
}

#[derive(Deserialize, Debug)]
struct SleeperUser {
    user_id: String,
    display_name: String,
    metadata: SleeperTeamMetadata,
    avatar: Option<String>,
}

#[derive(Clone, Deserialize, Debug)]
struct SleeperRoster {
    roster_id: u32,
    owner_id: String,
}

#[derive(Deserialize, Debug)]
struct NflPlayer {
    player_id: String,
    first_name: String,
    last_name: String,
    status: Option<String>,
    injury_status: Option<String>,
    injury_start_date: Option<String>,
    team: Option<String>,
}

struct Cache {
    player_map: HashMap<String, NflPlayer>,
    roster_map: HashMap<String, u32>,
    users_map: HashMap<String, SleeperUser>,
}

pub struct SleeperClient {
    league_id: String,
    cache: RwLock<Cache>,
}

impl SleeperClient {
    pub async fn new(league_id: String) -> SleeperClient {
        let cache = Cache {
            player_map: HashMap::new(),
            roster_map: HashMap::new(),
            users_map: HashMap::new(),
        };
        let mut client = SleeperClient {
            league_id: league_id.to_owned(),
            cache: RwLock::new(cache),
        };
        client.initialize().await;
        client
    }

    pub async fn get_league_details(&self) -> Result<(), Box<dyn Error>> {
        debug!("Requesting Sleeper league details...");
        let url = format!("{}/league/{}", SLEEPER_API_URL, self.league_id);
        let resp = reqwest::get(url).await?.json::<serde_json::Value>().await?;
        debug!("Got Sleeper league details:\n{:?}", resp);
        Ok(())
    }

    async fn initialize(&mut self) {
        let mut cache = self.cache.write().unwrap();
        self.load_players(&mut *cache)
            .await
            .expect("Could not initialize players list");
        self.load_teams(&mut *cache)
            .await
            .expect("Could not initialize teams list");
    }

    async fn load_players(&self, cache: &mut Cache) -> Result<(), Box<dyn Error>> {
        debug!("Loading Sleeper players map...");
        // Sleeper asks that we only call this endpoint once a day so let's be nice and do that.
        let reload = fs::metadata(PLAYERS_DATA_PATH)
            .ok()
            .map_or(true, |metadata| match metadata.modified() {
                Ok(mt) => mt
                    .elapsed()
                    .ok()
                    .map_or(true, |el| el.as_secs() > SECS_PER_DAY),
                Err(e) => {
                    eprintln!("Could not determine modification time. Err:\n{}", e);
                    true
                }
            });

        if reload {
            debug!("Reloading players file from Sleeper");
            self.fetch_players().await?;
        }

        self.load_players_from_file(cache);

        Ok(())
    }

    async fn fetch_players(&self) -> Result<(), Box<dyn Error>> {
        let url = format!("{}/players/nfl", SLEEPER_API_URL);
        let resp = reqwest::get(url).await?;
        let data_file = File::create(PLAYERS_DATA_PATH)?;
        let mut f = BufWriter::new(data_file);
        f.write_all(resp.text().await?.as_bytes())?;
        Ok(())
    }

    fn load_players_from_file(&self, cache: &mut Cache) {
        let data = fs::read_to_string(PLAYERS_DATA_PATH).expect("Unable to read data file");
        let mut json: HashMap<String, NflPlayer> =
            serde_json::from_str(&data).expect("Unable to parse data file");
        debug!("loaded {} players", json.len());
        cache.player_map.clear();
        for (key, val) in json.drain() {
            cache.player_map.insert(key, val);
        }
    }

    async fn load_teams(&self, cache: &mut Cache) -> Result<(), Box<dyn Error>> {
        let rosters_url = format!("{}/league/{}/rosters", SLEEPER_API_URL, self.league_id);
        let rosters_resp = reqwest::get(rosters_url)
            .await?
            .json::<Vec<SleeperRoster>>()
            .await?;
        let users_url = format!("{}/league/{}/users", SLEEPER_API_URL, self.league_id);
        let users_resp = reqwest::get(users_url)
            .await?
            .json::<Vec<SleeperUser>>()
            .await?;
        debug!("rosters: {:?}, users: {:?}", rosters_resp, users_resp);
        let mut rosters_by_user: HashMap<String, SleeperRoster> = HashMap::new();
        for roster in rosters_resp {
            rosters_by_user.insert(roster.owner_id.to_owned(), roster);
        }

        cache.users_map.clear();
        cache.roster_map.clear();

        for user in users_resp {
            let roster = rosters_by_user.remove(&user.user_id).unwrap();
            let user_id = user.user_id.clone();
            cache.users_map.insert(user_id.clone().to_owned(), user);
            cache
                .roster_map
                .insert(user_id.clone().to_owned(), roster.roster_id);
        }

        Ok(())
    }

    pub async fn get_nfl_state(&self) -> Result<SleeperNflStateApiResponse, Box<dyn Error>> {
        let state_url = format!("{}/state/nfl", SLEEPER_API_URL);
        let state_resp = reqwest::get(state_url)
            .await?
            .json::<SleeperNflStateApiResponse>()
            .await?;

        Ok(state_resp)
    }
}

#[async_trait]
impl super::FantasyClient for SleeperClient {
    async fn get_teams(&self) -> Result<Vec<super::FantasyTeam>, Box<dyn Error>> {
        let mut teams: Vec<super::FantasyTeam> = vec![];

        let cache = self.cache.read().unwrap();
        for sleeper_team in cache.users_map.values() {
            let team = super::FantasyTeam {
                id: sleeper_team.user_id.clone(),
                team_name: sleeper_team
                    .metadata
                    .team_name
                    .clone()
                    .unwrap_or(sleeper_team.display_name.clone()),
                owner_name: sleeper_team.display_name.clone(),
            };
            teams.push(team);
        }

        Ok(teams)
    }

    async fn get_matchups(
        &self,
        week_num: Option<u32>,
    ) -> Result<Vec<super::FantasyMatchup>, Box<dyn Error>> {
        let matchups: Vec<super::FantasyMatchup> = vec![];
        let req_week_num = match week_num {
            Some(n) => n,
            None => self.get_nfl_state().await?.week,
        };
        Ok(matchups)
    }
}
