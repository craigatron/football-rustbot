use async_trait::async_trait;
use serde::Deserialize;
use std::error::Error;
use std::option::Option;

pub mod espn;
pub mod sleeper;

pub struct FflClient {
    pub config: LeagueConfig,
    pub client_type: FflClientType,
}

pub enum FflClientType {
    ESPN(espn::EspnClient),
    SLEEPER(sleeper::SleeperClient),
}

#[derive(Clone)]
pub struct FantasyTeam {
    id: String,
    team_name: String,
    owner_name: String,
}

pub struct FantasyMatchup {
    team1: FantasyTeam,
    team2: FantasyTeam,
    score1: Option<f64>,
    score2: Option<f64>,
    week_num: u32,
}

#[derive(Clone, Deserialize, Debug, PartialEq, Eq, Hash)]
pub enum LeagueType {
    ESPN,
    SLEEPER,
}

#[derive(Clone, Deserialize, Debug)]
pub struct LeagueConfig {
    pub league_name: String,
    pub league_type: LeagueType,
    pub league_id: String,
    pub discord_category_id: String,
    pub short_name: String,
}

#[async_trait]
trait FantasyClient {
    async fn get_teams(&self) -> Result<Vec<FantasyTeam>, Box<dyn Error>>;
    async fn get_matchups(
        &self,
        week_num: Option<u32>,
    ) -> Result<Vec<FantasyMatchup>, Box<dyn Error>>;
}
