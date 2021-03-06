import asyncio
import datetime
import json
import logging
import os
import discord
from espn_api.football import League
from google.cloud import storage

logging.getLogger().setLevel(logging.INFO)

ESPN_S2 = os.environ['ESPN_S2']
ESPN_SWID = os.environ['ESPN_SWID']
GCS_BUCKET = os.environ['GCS_BUCKET']
LEAGUE_CONFIG = os.environ['LEAGUE_CONFIG']
DISCORD_BOT_ID = os.environ['DISCORD_BOT_ID']
ACTIVITY_FILENAME_FORMAT = 'activity_{}_{}.json'


def update_recent_activity(event, context):
    asyncio.run(_update_recent_activity())


async def _update_recent_activity():
    # load leagues
    # [{'id': _ID_, 'type': 'espn'|'sleeper'},...]
    league_config = json.loads(LEAGUE_CONFIG)

    storage_client = storage.Client()
    bucket = storage_client.bucket(GCS_BUCKET)

    logging.info('starting client')
    client = discord.Client()

    @client.event
    async def on_ready():
        for league in league_config:
            if league['type'] != 'espn':
                logging.error(
                    f"Can't handle league with type: {league['type']}")
                continue

        _update_espn_activity(bucket, client, league['id'],
                              league.get('channel'))
        await client.close()
        logging.info('exiting')

    await client.start(DISCORD_BOT_ID)


async def _update_espn_activity(bucket, discord_client, league_id, channel_id):
    blob = bucket.blob(ACTIVITY_FILENAME_FORMAT.format('espn', league_id))
    if blob.exists():
        activity_json = json.loads(blob.download_as_bytes())
    else:
        activity_json = {'activity': []}

    league = League(int(league_id), 2021, espn_s2=ESPN_S2, swid=ESPN_SWID)
    recents = league.recent_activity(size=100)

    all_activity = activity_json['activity']

    newest = all_activity[0]['date'] if all_activity else None
    new_additions = [a for a in recents if newest is None or a.date > newest]

    for n in new_additions:
        all_activity.append({
            'date':
            n.date,
            'actions': [{
                'team': a[0].team_name,
                'action': a[1],
                'player': {
                    'name': a[2].name,
                    'team': a[2].proTeam,
                },
            } for a in n.actions]
        })
    all_activity.sort(key=lambda a: a['date'], reverse=True)

    activity_json['updated'] = datetime.datetime.now().isoformat()

    blob.upload_from_string(json.dumps(activity_json))
    blob.make_public()

    logging.info(f'new additions:\n{new_additions}')
    if new_additions and channel_id:
        channel = await discord_client.fetch_channel(channel_id)