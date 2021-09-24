import json
import logging
import os
from espn_api.football import League
from google.cloud import storage

logging.getLogger().setLevel(logging.INFO)

ESPN_S2 = os.environ['ESPN_S2']
ESPN_SWID = os.environ['ESPN_SWID']
GCS_BUCKET = os.environ['GCS_BUCKET']
LEAGUE_CONFIG = os.environ['LEAGUE_CONFIG']
POWER_FILENAME = 'power.json'


def update_power(event, context):
    # load leagues
    # [{'id': _ID_, 'type': 'espn'|'sleeper'},...]
    league_config = json.loads(LEAGUE_CONFIG)

    power_rankings = {}
    for league in league_config:
        league_power = _calculate_league_power(league['id'], league['type'])
        power_rankings[f'{league["type"]}_{league["id"]}'] = league_power

    storage_client = storage.Client()
    bucket = storage_client.bucket(GCS_BUCKET)
    blob = bucket.blob(POWER_FILENAME)
    blob.upload_from_string(json.dumps(power_rankings))
    blob.make_public()


def _calculate_league_power(league_id, league_type):
    if league_type == 'espn':
        league = League(int(league_id), 2021, espn_s2=ESPN_S2, swid=ESPN_SWID)
        power = league.power_rankings()
        logging.info(f'got espn power: {power}')
        return [{'power': p, 'team': t.team_name} for (p, t) in power]

    raise Exception('dunno how to do sleeper yet')