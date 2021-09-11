use async_trait::async_trait;
use http::{header::COOKIE, HeaderMap, HeaderValue};
use reqwest;
use serde::Deserialize;
use std::error::Error;
use std::option::Option;

// TODO: doesn't support older seasons.  but the present is all that matters
const ESPN_API_URL: &str = "https://fantasy.espn.com/apis/v3/games/ffl/seasons";
const LEAGUE_API_PATH: &str = "segments/0/leagues";

#[derive(Deserialize, Debug)]
struct EspnApiMembersResponse {
    members: Vec<EspnMember>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EspnMember {
    display_name: String,
    id: String,
}

pub struct EspnClient {
    league_id: u64,
    year: u32,
    espn_s2: String,
    swid: String,
}

impl EspnClient {
    pub fn new(league_id: u64, year: u32, espn_s2: String, swid: String) -> EspnClient {
        EspnClient {
            league_id,
            year,
            espn_s2,
            swid,
        }
    }

    async fn send_request(&self, views: Vec<String>) -> serde_json::Value {
        let mut headers = HeaderMap::new();
        // TODO: this assumes leagues are private, because all of mine are
        headers.insert(
            COOKIE,
            HeaderValue::from_str(format!("SWID={}; espn_s2={}", self.swid, self.espn_s2).as_str())
                .unwrap(),
        );
        let client = reqwest::Client::new();
        let mut req = client
            .get(format!(
                "{}/{}/{}/{}",
                ESPN_API_URL, self.year, LEAGUE_API_PATH, self.league_id
            ))
            .headers(headers);
        for view in views {
            req = req.query(&[("view", view)]);
        }
        println!("sending request:\n{:?}", req);
        let resp = req
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        resp
    }
}

#[async_trait]
impl super::FantasyClient for EspnClient {
    async fn get_teams(&self) -> Result<Vec<super::FantasyTeam>, Box<dyn Error>> {
        let teams_resp = self.send_request(vec!["mTeams".to_string()]).await;
        let resp: EspnApiMembersResponse = serde_json::from_value(teams_resp).unwrap();
        println!("got resp:\n{:?}", resp);
        let teams: Vec<super::FantasyTeam> = vec![];

        Ok(teams)
    }

    async fn get_matchups(
        &self,
        week_num: Option<u32>,
    ) -> Result<Vec<super::FantasyMatchup>, Box<dyn Error>> {
        let matchups: Vec<super::FantasyMatchup> = vec![];
        Ok(matchups)
    }
}
