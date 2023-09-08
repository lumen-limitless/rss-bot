use crate::{Context, Error};

/// Show this help menu
#[poise::command(slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), Error> {
    poise::builtins::help(
        ctx,
        command.as_deref(),
        poise::builtins::HelpConfiguration {
            extra_text_at_bottom: "This discord bot fetches the latest news from NEWS_URL env var and sends a message with the link to the channel with id CHANNEL_ID at specified intervals.",
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}
