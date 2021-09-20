import datetime
import json
import logging
import os
import pytz
import requests
from google.cloud import storage

SLEEPER_PLAYERS_API = 'https://api.sleeper.app/v1/players/nfl'
ALL_PLAYERS_FILENAME = 'sleeper_players.json'
COVID_PLAYERS_FILENAME = 'covid_players.json'
DATED_COVID_PLAYERS_FORMAT = 'covid_players_{}.json'
GCS_BUCKET = os.environ['GCS_BUCKET']

logging.getLogger().setLevel(logging.INFO)


def update_sleeper(event, context):
    storage_client = storage.Client()
    bucket = storage_client.bucket(GCS_BUCKET)
    blob = bucket.blob(ALL_PLAYERS_FILENAME)
    if blob.exists():
        logging.info(f'last updated: {blob.updated}')

    players = requests.get(SLEEPER_PLAYERS_API).json()
    blob.upload_from_string(json.dumps(players))

    covid_players = {
        k: v
        for k, v in players.items() if v.get('injury_status') == 'COV'
    }

    now = datetime.datetime.now(pytz.timezone('America/New_York'))
    today = now.date()
    logging.info(f'timezone: {now.tzinfo}')
    yesterday = today - datetime.timedelta(days=1)
    yesterday_covid_blob = bucket.blob(
        DATED_COVID_PLAYERS_FORMAT.format(yesterday.isoformat()))
    if yesterday_covid_blob.exists():
        yesterday_covid_players = json.loads(
            yesterday_covid_blob.download_as_bytes())
    else:
        yesterday_covid_players = {}

    for k, v in covid_players.items():
        if k in yesterday_covid_players:
            start_date = yesterday_covid_players[k].get('start_date')
            if start_date:
                v['start_date'] = start_date
            elif v.get('new'):
                v['start_date'] = yesterday.isoformat()
            else:
                v['start_date'] = 'unk'
        else:
            v['start_date'] = today.isoformat()

    # Sleeper's API apparently doesn't include an injury start date for COVID
    # so we'll just have to infer it by the date we generated the file
    file_str = json.dumps(covid_players)
    covid_blob = bucket.blob(COVID_PLAYERS_FILENAME)
    covid_blob.upload_from_string(file_str)

    dated_covid_blob = bucket.blob(
        DATED_COVID_PLAYERS_FORMAT.format(today.isoformat()))
    dated_covid_blob.upload_from_string(file_str)
