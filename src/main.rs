#![feature(is_some_with)]
use std::collections::{HashMap};
//use std::env;
use std::sync::Arc;
use std::io::Write;
use std::fs::File;
use std::fs;
use std::process::Command;
use std::io::Read;

use serenity::async_trait;
use serenity::client::bridge::gateway::{ShardManager};
//use serenity::framework::standard::buckets::{LimitedFor, RevertBucket};
use serenity::framework::standard::macros::{command, group, hook};
use serenity::framework::standard::{
    Args,
    CommandResult,
    StandardFramework,
};
use serenity::http::Http;
use serenity::model::channel::{Message};
use serenity::model::gateway::{GatewayIntents, Ready};
//use serenity::model::id::UserId;
//use serenity::model::permissions::Permissions;
use serenity::prelude::*;
use serenity::utils::{content_safe, ContentSafeOptions};
use tokio::sync::Mutex;


struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

struct CommandCounter;

impl TypeMapKey for CommandCounter {
    type Value = HashMap<String, u64>;
}

#[hook]
async fn before(ctx: &Context, msg: &Message, command_name: &str) -> bool {
    println!("Got command '{}' by user '{}'", command_name, msg.author.name);

    // Increment the number of times this command has been run once. If
    // the command's name does not exist in the counter, add a default
    // value of 0.
    let mut data = ctx.data.write().await;
    let counter = data.get_mut::<CommandCounter>().expect("Expected CommandCounter in TypeMap.");
    let entry = counter.entry(command_name.to_string()).or_insert(0);
    *entry += 1;

    true // if `before` returns false, command processing doesn't happen.
}

#[hook]
async fn after(_ctx: &Context, _msg: &Message, command_name: &str, command_result: CommandResult) {
    match command_result {
        Ok(()) => println!("Processed command '{}'", command_name),
        Err(why) => println!("Command '{}' returned error {:?}", command_name, why),
    }
}

#[hook]
async fn unknown_command(_ctx: &Context, _msg: &Message, unknown_command_name: &str) {
    println!("Could not find command named '{}'", unknown_command_name);
}

#[hook]
async fn normal_message(_ctx: &Context, msg: &Message) {
    for attachment in &msg.attachments {
        if attachment.height.is_none() && attachment.width.is_none() && attachment.content_type.is_some_and(|x| x.starts_with("video/")) {
            let content = match attachment.download().await {
                Ok(content) => {
                    content
                },
                Err(why) => {
                    println!("Error downloading attachment: {:?}", why);
                    return;
                },
            };

            let fpath = &attachment.id.as_u64().to_string();
            let opath = &mut attachment.id.as_u64().to_string();
            opath.push_str(".mp4");
            let mut file = File::create(fpath).unwrap();
            file.write_all(content.as_slice()).unwrap();

            let encodestatus = Command::new("cmd")
                .args(["/C", "ffmpeg", "-i", fpath, "-c:v", "libx264", opath])
                .status()
                .expect("ffmpeg failed to start");

            fs::remove_file(fpath).unwrap();
            if encodestatus.success() {
                println!("Sending reencoded version");
                
                msg.channel_id.send_message(&_ctx.http, |m| {
                    m
                        .content("Discord-friendly version: ")
                        .reference_message(msg)
                        .add_file(opath.as_str())
                }).await.unwrap();//
                fs::remove_file(opath).unwrap();
                
                //msg.channel_id.send_files(&_ctx.http, f2, |m| m.content("Discord-friendly version: ")).await;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Login with a bot token from the environment
    let mut secret = File::open("secret").unwrap();
    let mut token = String::new();
    secret.read_to_string(&mut token).unwrap();
    let http = Http::new(&token);

    let framework = StandardFramework::new()
        .configure(|c| c
            .prefix("!")
        )
        .before(before)
        .after(after)
        .unrecognised_command(unknown_command)
        .normal_message(normal_message)
        .group(&GENERAL_GROUP);


    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::DIRECT_MESSAGES;
    let mut client = Client::builder(token, intents)
        .event_handler(Handler)
        .framework(framework)
        .type_map_insert::<CommandCounter>(HashMap::default())
        .await
        .expect("Error creating client");
    
    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
    }

    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}
#[group]
#[commands(say)]
struct General;

#[command]
async fn say(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let settings = if let Some(guild_id) = msg.guild_id {
        // By default roles, users, and channel mentions are cleaned.
        ContentSafeOptions::default()
            // We do not want to clean channal mentions as they
            // do not ping users.
            .clean_channel(false)
            // If it's a guild channel, we want mentioned users to be displayed
            // as their display name.
            .display_as_member_from(guild_id)
    } else {
        ContentSafeOptions::default().clean_channel(false).clean_role(false)
    };

    let content = content_safe(&ctx.cache, &args.rest(), &settings, &msg.mentions);

    msg.channel_id.say(&ctx.http, &content).await?;

    Ok(())
}