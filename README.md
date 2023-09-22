## Discord News Bot

This discord bot fetches the latest artcicle from an rss feed, specified by the `RSS_URL` env var, then sends a message with the link to the channel with id `CHANNEL_ID` env var if there is a new article at 60 second intervals.
