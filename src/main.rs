#![warn(clippy::str_to_string)]

mod commands;

use ::rss::Channel;
use poise::serenity_prelude::{self as serenity, ChannelId, GetMessages};
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
        poise::FrameworkError::Command { error, ctx, .. } => {
            tracing::error!("Error in command `{}`: {:?}", ctx.command().name, error,);
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                tracing::error!("Error while handling error: {}", e)
            }
        }
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt::init();

    let token = std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");

    let channel_id = std::env::var("CHANNEL_ID").expect("missing CHANNEL_ID");

    let rss_url = var("RSS_URL").expect("Missing RSS_URL env var");

    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;

    // FrameworkOptions contains all of poise's configuration option in one struct
    // Every option can be omitted to use its default value
    let options = poise::FrameworkOptions {
        commands: vec![commands::help::help()],

        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("~".into()),
            additional_prefixes: vec![
                poise::Prefix::Literal("hey bot"),
                poise::Prefix::Literal("hey bot,"),
            ],
            ..Default::default()
        },
        // The global error handler for all error cases that may occur
        on_error: |error| Box::pin(on_error(error)),
        // This code is run before every command
        pre_command: |ctx| {
            Box::pin(async move {
                tracing::info!("Executing command {}...", ctx.command().qualified_name);
            })
        },
        // This code is run after a command if it was successful (returned Ok)
        post_command: |ctx| {
            Box::pin(async move {
                tracing::info!("Executed command {}!", ctx.command().qualified_name);
            })
        },
        // Every command invocation must pass this check to continue execution
        command_check: Some(|_ctx| Box::pin(async move { Ok(true) })),
        // Enforce command checks even for owners (enforced by default)
        // Set to true to bypass checks, which is useful for testing
        skip_checks_for_owners: false,
        event_handler: |_ctx, event, _framework, _data| {
            Box::pin(async move {
                tracing::info!(
                    "Got an event in event handler: {:?}",
                    event.snake_case_name()
                );
                Ok(())
            })
        },
        ..Default::default()
    };

    let framework = poise::Framework::builder()
        .options(options)
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                let shared_ctx = Arc::new(ctx.clone());

                tracing::info!("Logged in as {}", ready.user.name);
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                let sched = JobScheduler::new().await?;

                let rss_url = Arc::new(rss_url);
                let channel_id = Arc::new(channel_id);

                let news_update_job = Job::new_repeated_async(Duration::from_secs(60), {
                    let shared_ctx = shared_ctx.clone();
                    let rss_url = rss_url.clone();
                    let channel_id = channel_id.clone();

                    move |_uuid, _| {
                        let ctx = shared_ctx.clone();
                        let rss_url = rss_url.clone();
                        let channel_id = channel_id.clone();

                        Box::pin(async move {
                            if let Err(e) = fetch_and_post_news(&ctx, &rss_url, &channel_id).await {
                                tracing::error!("Error in news update job: {}", e);
                            }
                        })
                    }
                })
                .unwrap();

                sched.add(news_update_job).await?;

                sched.start().await?;

                Ok(Data {})
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await?;

    tracing::info!("Starting bot...");
    client.start().await?;

    Ok(())
}

async fn fetch_and_post_news(
    ctx: &Arc<serenity::Context>,
    rss_url: &str,
    channel_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let res = reqwest::get(rss_url).await?;
    let content = res.bytes().await?;
    let content_channel = Channel::read_from(&content[..])?;

    let story = content_channel.items.first().ok_or("No items in feed")?;
    let story_link = story.link.as_ref().ok_or("No link found")?;

    let channel_id = ChannelId::from(channel_id.parse::<u64>()?);
    let channel = channel_id
        .to_channel(&ctx)
        .await?
        .guild()
        .ok_or("Not a guild channel")?;

    let prev_news = channel.messages(&ctx, GetMessages::default()).await?;

    if let Some(m) = prev_news.first() {
        if m.content == *story_link {
            tracing::info!("No new articles");
            return Ok(());
        }
    }

    channel_id.say(ctx, story_link).await?;
    tracing::info!("Posted new article");

    Ok(())
}
