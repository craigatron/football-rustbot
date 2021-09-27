use football_rustbot::discord_client;
use football_rustbot::fantasy_client::espn;
use football_rustbot::fantasy_client::sleeper;
use football_rustbot::fantasy_client::{FflClient, FflClientType, LeagueConfig, LeagueType};
use serde::Deserialize;
use std::fs::File;

const CONFIG_FILE: &str = "config.json";

#[derive(Deserialize, Debug)]
struct DiscordConfig {
    app_id: u64,
    bot_token: String,
    ignore_reaccs: Vec<String>,
    covid_json_url: String,
    power_ranking_url_format: String,
}

#[derive(Deserialize, Debug)]
struct EspnConfig {
    swid: String,
    s2: String,
}

#[derive(Deserialize, Debug)]
struct Config {
    discord_config: DiscordConfig,
    espn_config: EspnConfig,
    leagues: Vec<LeagueConfig>,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let config = load_config();

    let mut ffl_clients: Vec<FflClient> = vec![];
    for league_config in config.leagues {
        let league_id = league_config.league_id.clone();
        let ffl_client = match league_config.league_type {
            LeagueType::ESPN => FflClient {
                config: league_config.clone(),
                client_type: FflClientType::ESPN(espn::EspnClient::new(
                    league_config.league_id.clone().parse::<u64>().unwrap(),
                    2021,
                    config.espn_config.s2.clone(),
                    config.espn_config.swid.clone(),
                )),
            },
            LeagueType::SLEEPER => FflClient {
                config: league_config,
                client_type: FflClientType::SLEEPER(sleeper::SleeperClient::new(league_id).await),
            },
        };
        ffl_clients.push(ffl_client);
    }

    let mut client = discord_client::DiscordClient::new(
        config.discord_config.bot_token,
        config.discord_config.app_id,
        config.discord_config.ignore_reaccs,
        ffl_clients,
        config.discord_config.covid_json_url,
        config.discord_config.power_ranking_url_format,
    )
    .await;
    client.start().await.expect("client error");
}

fn load_config() -> Config {
    let file = File::open(CONFIG_FILE).expect("Could not open config file");
    let config: Config = serde_json::from_reader(file).expect("Could not parse config");
    println!("parsed config: {:?}", config);
    config
}
