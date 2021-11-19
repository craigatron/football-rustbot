use super::fantasy_client::{FflClient, LeagueType};
use phf::phf_map;
use serde::Deserialize;
use serenity::{
    async_trait,
    model::{
        channel::Message,
        gateway::Ready,
        interactions::{
            application_command::{ApplicationCommand, ApplicationCommandOptionType},
            Interaction, InteractionResponseType,
        },
    },
    prelude::*,
};
use std::collections::HashMap;
use std::option::Option;

static REACC_MAP: phf::Map<&str, char> = phf_map! {
    "football" => '🏈',
    "butt" => '🍑',
};

pub struct DiscordClient {
    client: Client,
}

#[derive(Deserialize, Debug)]
struct CovidPlayer {
    full_name: String,
    team: String,
    start_date: Option<String>,
    search_rank: Option<u64>,
}

#[derive(Deserialize, Debug)]
struct PowerResponse {
    power: Vec<TeamPower>,
    updated: String,
}

#[derive(Deserialize, Debug)]
struct TeamPower {
    power: String,
    team: String,
}

impl DiscordClient {
    pub async fn new(
        token: String,
        app_id: u64,
        ignore_reaccs: Vec<String>,
        ffl_clients: Vec<FflClient>,
        covid_json_url: String,
        power_ranking_url_format: String,
    ) -> DiscordClient {
        let handler = Handler {
            ignore_reaccs,
            ffl_clients,
            covid_json_url,
            power_ranking_url_format,
        };
        let client = Client::builder(token)
            .event_handler(handler)
            .application_id(app_id)
            .await
            .expect("Error creating client");
        DiscordClient { client }
    }

    pub async fn start(&mut self) -> Result<(), serenity::Error> {
        self.client.start().await
    }
}

struct Handler {
    ignore_reaccs: Vec<String>,
    ffl_clients: Vec<FflClient>,
    covid_json_url: String,
    power_ranking_url_format: String,
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        println!("got interaction: {:?}", interaction);
        if let Interaction::ApplicationCommand(slash_command) = interaction {
            let command = slash_command.data.name.as_str();
            let mut reply: Option<String> = None;
            if command == "whosgotcovid" {
                reply = self.handle_whosgotcovid().await;
            } else {
                let mut league_name: Option<String> = None;
                for option in slash_command.data.options.iter() {
                    if option.name == "league" {
                        league_name = option.value.clone().map(|v| v.as_str().unwrap().to_owned());
                        break;
                    }
                }
                println!(
                    "received slash command: {:?} for league {:?}",
                    slash_command, league_name,
                );
                println!("channel ID: {:?}", slash_command.channel_id);
                let ffl_client = match league_name {
                    Some(n) => self.get_client_by_name(n),
                    None => {
                        // what the shit is this I just want to get the category ID
                        let category = slash_command
                            .channel_id
                            .to_channel(&ctx.http)
                            .await
                            .unwrap()
                            .guild()
                            .unwrap()
                            .category_id
                            .unwrap()
                            .to_channel(&ctx.http)
                            .await
                            .unwrap()
                            .category();
                        println!("attempting to get league from category {:?}", category);
                        let category_id = category.unwrap().id.as_u64().to_string();
                        self.get_client_by_category_id(category_id)
                    }
                }
                .unwrap();

                reply = match command {
                    "matchups" => self.handle_matchups().await,
                    "standings" => self.handle_standings().await,
                    "power" => self.handle_power(&ffl_client).await,
                    _ => None,
                };
            }

            let answer = reply.unwrap();
            println!("replying with message {}", answer);
            if let Err(e) = slash_command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| message.content(answer))
                })
                .await
            {
                println!("failed to respond to slash command: {}", e);
            }
        }
    }

    async fn message(&self, ctx: Context, message: Message) {
        println!("message received: {:?}", message);
        if self
            .ignore_reaccs
            .contains(&message.author.id.as_u64().to_string())
        {
            println!("not reaccing message");
            return;
        }
        for (key, value) in REACC_MAP.into_iter() {
            if message.content.to_ascii_lowercase().contains(key) {
                message.react(&ctx.http, *value).await.unwrap();
            }
            if message.content.to_ascii_lowercase().contains("69") {
                message.react(&ctx.http, '🇳').await.unwrap();
                message.react(&ctx.http, '🇮').await.unwrap();
                message.react(&ctx.http, '🇨').await.unwrap();
                message.react(&ctx.http, '🇪').await.unwrap();
            }
        }
        for m in message.mentions.iter() {
            if m.bot {
                message.react(&ctx.http, '🤖').await.unwrap();
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let commands = ApplicationCommand::set_global_application_commands(&ctx.http, |commands| {
            for command_config in vec![
                ("matchups", "Fetch this week's matchups"),
                ("standings", "Fetch the current standings"),
                ("power", "Fetch power rankings"),
            ] {
                commands.create_application_command(|command| {
                    command
                        .name(command_config.0)
                        .description(command_config.1)
                        .create_option(|option| {
                            for client in self.ffl_clients.iter() {
                                option.add_string_choice(
                                    client.config.league_name.clone(),
                                    client.config.short_name.clone(),
                                );
                            }
                            option
                                .kind(ApplicationCommandOptionType::String)
                                .name("league")
                                .description("which league?")
                        })
                });
            }
            commands.create_application_command(|command| {
                command
                    .name("whosgotcovid")
                    .description("the COVID naughty list")
            });
            println!("trying to create commands: {:?}", commands);
            commands
        })
        .await
        .unwrap();

        println!("Created slash commands: {:#?}", commands);

        //let channel = ctx.http.get_channel(BOT_CHANNEL).await.unwrap();
        //channel.id().send_message(ctx, |m| {
        //    m.content("TESTING 1 2 3")
        //}).await;
    }
}

impl Handler {
    fn get_client_by_category_id(&self, id: String) -> Option<&FflClient> {
        let mut ret: Option<&FflClient> = None;
        for client in self.ffl_clients.iter() {
            if client.config.discord_category_id == id {
                ret = Some(&client);
                break;
            }
        }
        ret
    }

    fn get_client_by_name(&self, name: String) -> Option<&FflClient> {
        let mut ret: Option<&FflClient> = None;
        for client in self.ffl_clients.iter() {
            if client.config.short_name == name {
                ret = Some(&client);
                break;
            }
        }
        ret
    }

    async fn handle_matchups(&self) -> Option<String> {
        Some(
            "```
this is a matchups response
```"
            .to_string(),
        )
    }

    async fn handle_standings(&self) -> Option<String> {
        Some(
            "```
this is a standings response
```"
            .to_string(),
        )
    }

    async fn handle_whosgotcovid(&self) -> Option<String> {
        let covid_resp = reqwest::get(&self.covid_json_url)
            .await
            .unwrap()
            .json::<HashMap<String, CovidPlayer>>()
            .await
            .unwrap();
        let mut covid_players = vec![];
        for player in covid_resp.values() {
            if player.search_rank.unwrap_or(9999999) < 9999999 {
                let start_date = match player.start_date.clone() {
                    Some(d) => format!(" ({})", d),
                    None => "".to_string(),
                };
                covid_players.push(format!(
                    "{}, {}{}",
                    player.full_name, player.team, start_date
                ))
            }
        }
        if covid_players.len() == 0 {
            Some(
                "```
Nobody, apparently.
```"
                .to_string(),
            )
        } else {
            Some(format!("```{}```", covid_players.join("\n")))
        }
    }

    async fn handle_power(&self, ffl_client: &FflClient) -> Option<String> {
        let id = &ffl_client.config.league_id;
        let league_type = match ffl_client.config.league_type {
            LeagueType::SLEEPER => "sleeper",
            LeagueType::ESPN => "espn",
            _ => panic!("wtf league type is that"),
        };
        println!("getting power for league {} of type {}", id, league_type);

        if league_type == "sleeper" {
            Some("power rankings for sleeper not implemented yet sorryyyyyyyy".to_string())
        } else {
            let url = self
                .power_ranking_url_format
                .clone()
                .replace("<LEAGUE_ID>", id)
                .replace("<LEAGUE_TYPE>", league_type);
            println!("fetching power from URL {}", url);
            let power_resp = reqwest::get(url)
                .await
                .unwrap()
                .json::<PowerResponse>()
                .await
                .unwrap();
            let mut lines = vec![];
            for team in power_resp.power.iter() {
                lines.push(format!("{} ({})", team.team, team.power))
            }
            lines.push(format!("\nupdated {}", power_resp.updated));

            Some(format!(
                "```
{}
```",
                lines.join("\n")
            ))
        }
    }
}
