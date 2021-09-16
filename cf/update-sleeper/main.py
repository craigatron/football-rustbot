import datetime
import json
import logging
import os
import requests
from google.cloud import storage

SLEEPER_PLAYERS_API = 'https://api.sleeper.app/v1/players/nfl'
ALL_PLAYERS_FILENAME = 'sleeper_players.json'
COVID_PLAYERS_FILENAME = 'covid_players.json'
DATED_COVID_PLAYERS_FORMAT = 'covid_players_{}.json'
GCS_BUCKET = os.environ['GCS_BUCKET']


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

    today = datetime.date.today()
    yesterday_covid_blob = bucket.blob(
        DATED_COVID_PLAYERS_FORMAT.format(
            (today - datetime.timedelta(days=1)).isoformat()))
    if yesterday_covid_blob.exists():
        yesterday_covid_players = json.loads(
            yesterday_covid_blob.download_as_bytes())
    else:
        yesterday_covid_players = {}

    for k, v in covid_players.items():
        v['new'] = k not in yesterday_covid_players

    # Sleeper's API apparently doesn't include an injury start date for COVID
    # so we'll just have to infer it by the date we generated the file
    file_str = json.dumps(covid_players)
    covid_blob = bucket.blob(COVID_PLAYERS_FILENAME)
    covid_blob.upload_from_string(file_str)

    dated_covid_blob = bucket.blob(
        DATED_COVID_PLAYERS_FORMAT.format(today.isoformat()))
    dated_covid_blob.upload_from_string(file_str)
