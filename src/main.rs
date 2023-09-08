#![warn(clippy::str_to_string)]

mod commands;

use ::rss::Channel;
use poise::serenity_prelude::{self as serenity};
use std::{env::var, sync::Arc, time::Duration};
use tokio_cron_scheduler::{Job, JobScheduler};
// Types used by all command functions
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

// Custom user data passed to all command functions
pub struct Data {}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    // This is our custom error handler
    // They are many errors that can occur, so we only handle the ones we want to customize
    // and forward the rest to the default handler
    match error {
        poise::FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
        poise::FrameworkError::Command { error, ctx } => {
            println!("Error in command `{}`: {:?}", ctx.command().name, error,);
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                println!("Error while handling error: {}", e)
            }
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    // FrameworkOptions contains all of poise's configuration option in one struct
    // Every option can be omitted to use its default value
    let options = poise::FrameworkOptions {
        commands: vec![commands::help::help()],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("~".into()),
            edit_tracker: Some(poise::EditTracker::for_timespan(Duration::from_secs(3600))),
            additional_prefixes: vec![
                poise::Prefix::Literal("hey bot"),
                poise::Prefix::Literal("hey bot,"),
            ],
            ..Default::default()
        },
        /// The global error handler for all error cases that may occur
        on_error: |error| Box::pin(on_error(error)),
        /// This code is run before every command
        pre_command: |ctx| {
            Box::pin(async move {
                println!("Executing command {}...", ctx.command().qualified_name);
            })
        },
        /// This code is run after a command if it was successful (returned Ok)
        post_command: |ctx| {
            Box::pin(async move {
                println!("Executed command {}!", ctx.command().qualified_name);
            })
        },
        /// Every command invocation must pass this check to continue execution
        command_check: Some(|_ctx| Box::pin(async move { Ok(true) })),
        /// Enforce command checks even for owners (enforced by default)
        /// Set to true to bypass checks, which is useful for testing
        skip_checks_for_owners: false,
        event_handler: |_ctx, event, _framework, _data| {
            Box::pin(async move {
                println!("Got an event in event handler: {:?}", event.name());
                Ok(())
            })
        },
        ..Default::default()
    };

    poise::Framework::builder()
        .token(
            var("DISCORD_TOKEN")
                .expect("Missing `DISCORD_TOKEN` env var, see README for more information."),
        )
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                let shared_ctx = Arc::new(ctx.clone());

                println!("Logged in as {}", _ready.user.name);
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                let sched = JobScheduler::new().await?;
                // let ctx_clone = Arc::clone(&shared_ctx); // Clone the Arc for use inside the closure

                let news_update_job =
                    Job::new_repeated_async(Duration::from_secs(60), move |_uuid, _| {
                        let ctx = Arc::clone(&shared_ctx);

                        Box::pin(async move {
                            let news_url = var("NEWS_URL").expect("Missing NEWS_URL env var");

                            let res = reqwest::get(news_url).await;

                            if res.is_err() {
                                println!("Error getting news: {}", res.err().unwrap());
                                return;
                            }

                            let res = res.unwrap();

                            let content = res.bytes().await;

                            if content.is_err() {
                                println!("Error getting news: {}", content.err().unwrap());
                                return;
                            }

                            let content = content.unwrap();

                            let content_channel =
                                Channel::read_from(&content[..]).expect("Failed to parse RSS");

                            let story = content_channel.items[0].clone();

                            if story.link.is_none() {
                                println!("No link found");
                                return;
                            }

                            let story_link = story.link.unwrap();

                            let channel_id = serenity::ChannelId(
                                var("CHANNEL_ID")
                                    .expect("Missing CHANNEL_ID env var")
                                    .parse::<u64>()
                                    .expect("Failed to parse CHANNEL_ID env var"),
                            );

                            let prev_news = channel_id
                                .messages(&ctx, |retriever| retriever.limit(1))
                                .await;

                            if prev_news.is_err() {
                                println!(
                                    "Error getting previous news: {}",
                                    prev_news.err().unwrap()
                                );
                                return;
                            }

                            let prev_news = prev_news.unwrap();

                            if let Some(m) = prev_news.first() {
                                if m.content == story_link {
                                    println!("No new news");
                                    return;
                                }
                            }

                            match channel_id.say(ctx, story_link).await {
                                Ok(_) => println!("Posted news"),
                                Err(e) => println!("Error posting news: {}", e),
                            };
                        })
                    })
                    .unwrap();

                sched.add(news_update_job).await?;

                sched.start().await?;

                Ok(Data {})
            })
        })
        .options(options)
        .intents(
            serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT,
        )
        .run()
        .await
        .map_err(|e| {
            println!("Failed to start bot: {}", e);
            e
        })
        .unwrap();
}
