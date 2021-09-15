use super::fantasy_client::FflClient;
use phf::phf_map;
use serenity::{
    async_trait,
    model::{
        channel::Message,
        gateway::Ready,
        interactions::{
            application_command::{
                ApplicationCommand, ApplicationCommandInteraction, ApplicationCommandOptionType,
            },
            Interaction, InteractionResponseType,
        },
    },
    prelude::*,
};
use std::option::Option;

static REACC_MAP: phf::Map<&str, char> = phf_map! {
    "football" => '🏈',
    "butt" => '🍑',
};

pub struct DiscordClient {
    client: Client,
}

impl DiscordClient {
    pub async fn new(token: String, app_id: u64, ffl_clients: Vec<FflClient>) -> DiscordClient {
        let handler = Handler { ffl_clients };
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
    ffl_clients: Vec<FflClient>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        println!("got interaction: {:?}", interaction);
        if let Interaction::ApplicationCommand(slash_command) = interaction {
            let command = slash_command.data.name.as_str();
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
            let ffl_client = match league_name {
                Some(n) => self.get_client_by_name(n),
                None => {
                    let category = slash_command
                        .channel_id
                        .to_channel(&ctx.http)
                        .await
                        .unwrap()
                        .category();
                    let category_id = category.unwrap().id.as_u64().to_string();
                    self.get_client_by_category_id(category_id)
                }
            };

            let reply = match command {
                "matchups" => Some(self.handle_matchups(&ctx).await),
                "standings" => Some(self.handle_standings(&ctx).await),
                _ => None,
            };

            if let Err(e) = slash_command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| {
                            message.content(
                                "```
sorry craig is slow and this isn't ready yet
```",
                            )
                        })
                })
                .await
            {
                println!("failed to respond to slash command: {}", e);
            }
        }
    }

    async fn message(&self, ctx: Context, message: Message) {
        println!("message received: {:?}", message);
        for (key, value) in REACC_MAP.into_iter() {
            if message.content.to_ascii_lowercase().contains(key) {
                message.react(&ctx.http, *value).await.unwrap();
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

    async fn handle_matchups(&self, ctx: &Context) -> String {
        "```
this is a matchups response
```"
        .to_string()
    }

    async fn handle_standings(&self, ctx: &Context) -> String {
        "```
this is a standings response
```"
        .to_string()
    }
}
