import datetime
import json
import logging
import os
import requests
import tempfile
from google.cloud import datastore
from google.cloud import storage
from espn_api.football import League
from mako.template import Template
import plotly.graph_objects as go

logging.getLogger().setLevel(logging.INFO)

SLEEPER_TOKEN = os.environ.get('SLEEPER_TOKEN')
SLEEPER_LEAGUE_ID = os.environ.get('SLEEPER_LEAGUE_ID')
ESPN_S2 = os.environ.get('ESPN_S2')
ESPN_SWID = os.environ.get('ESPN_SWID')
ESPN_LEAGUES = os.environ.get('ESPN_LEAGUES')
PROJECTIONS_BUCKET = 'football-rustbot-projections'

SLEEPER_GRAPHQL = 'https://sleeper.app/graphql'
SLEEPER_API = 'https://api.sleeper.app/v1'


def update_projections(event, context):
    ds_client = datastore.Client()
    storage_client = storage.Client()
    bucket = storage_client.get_bucket(PROJECTIONS_BUCKET)
    _update_sleeper(ds_client, bucket)
    _update_espn(ds_client, bucket)


def _update_sleeper(ds_client, bucket):
    state = requests.get(f'{SLEEPER_API}/state/nfl').json()
    current_week = state['week']
    current_season = state['league_season']

    week_key = ds_client.key('leagues', SLEEPER_LEAGUE_ID, 'seasons',
                             current_season, 'weeks', str(current_week))
    entity = ds_client.get(week_key)
    if entity is None:
        entity = datastore.Entity(week_key)
        entity['projections'] = []

    now = int(datetime.datetime.now().timestamp())

    matchups = requests.get(
        f'https://api.sleeper.app/v1/league/{SLEEPER_LEAGUE_ID}/matchups/{current_week}'
    ).json()
    starters_by_roster = {m['roster_id']: m['starters'] for m in matchups}
    all_starters = [
        s for starters in starters_by_roster.values() for s in starters
    ]

    players_resp = _graphql(
        _get_player_stats_and_projections(current_week, all_starters))
    projected_stats_by_player = {
        p['player_id']: p
        for p in players_resp['data']['projected']
    }
    actual_stats_by_player = {
        p['player_id']: p
        for p in players_resp['data']['actual']
    }

    league_settings = requests.get(
        f'{SLEEPER_API}/league/{SLEEPER_LEAGUE_ID}').json()
    scoring_settings = league_settings['scoring_settings']

    games_by_id = _get_games_by_id(current_week)

    for matchup in matchups:
        projection = sum([
            _calculate_player_projection(scoring_settings,
                                         actual_stats_by_player.get(p),
                                         projected_stats_by_player.get(p),
                                         games_by_id)
            for p in matchup['starters']
        ])
        entity['projections'].append({
            'timestamp': now,
            'roster_id': matchup['roster_id'],
            'matchup_id': matchup['matchup_id'],
            'projection': projection
        })

    ds_client.put(entity)

    users = requests.get(
        f'{SLEEPER_API}/league/{SLEEPER_LEAGUE_ID}/users').json()
    rosters = requests.get(
        f'{SLEEPER_API}/league/{SLEEPER_LEAGUE_ID}/rosters').json()
    roster_id_to_user = {r['roster_id']: r['owner_id'] for r in rosters}
    user_id_to_team_name = {
        u['user_id']: u['metadata'].get('team_name') or u['display_name']
        for u in users
    }

    _write_html(bucket, SLEEPER_LEAGUE_ID, current_week, {
        rid: user_id_to_team_name[oid]
        for (rid, oid) in roster_id_to_user.items()
    }, entity['projections'], 'sleeper')


def _graphql(op_json):
    return requests.post(SLEEPER_GRAPHQL,
                         json=op_json,
                         headers={
                             'authorization': SLEEPER_TOKEN
                         }).json()


def _get_player_stats_and_projections(week, all_players):
    return {
        "operationName":
        "get_player_score_and_projections_batch",
        "variables": {},
        "query":
        f"query get_player_score_and_projections_batch {{ actual: stats_for_players_in_week(sport: \"nfl\",season: \"2021\",category: \"stat\",season_type: \"regular\",week: {week},player_ids: {json.dumps(all_players)}){{ game_id opponent player_id stats team week season }} projected: stats_for_players_in_week(sport: \"nfl\",season: \"2021\",category: \"proj\",season_type: \"regular\",week: {week},player_ids: {json.dumps(all_players)}){{ game_id opponent player_id stats team week season }} }}"
    }


def _get_gamedata_query(week):
    return {
        "operationName":
        "batch_scores",
        "variables": {},
        "query":
        f"query batch_scores {{scores: scores(sport: \"nfl\",season_type: \"regular\",season: \"2021\",week: {week}){{date game_id metadata season season_type sport status week start_time}}}}"
    }


def _calculate_player_projection(scoring_settings, actual_stats,
                                 projected_stats, games_by_id):
    if not projected_stats:
        return 0
    game_info = games_by_id[projected_stats['game_id']]
    original_projection = _score_stats(scoring_settings,
                                       projected_stats['stats'])
    if game_info['status'] == 'pre_game':
        return original_projection
    current_score = _score_stats(scoring_settings,
                                 actual_stats['stats']) if actual_stats else 0
    if game_info['status'] == 'complete':
        return current_score
    seconds_remaining = game_info['seconds_left']
    fractional_game_left = seconds_remaining / 3600
    minutes_remaining = seconds_remaining / 60
    # TODO: overtime?
    minutes_played = 60 - minutes_remaining
    s = current_score + (current_score / max(minutes_played, 1) *
                         minutes_remaining * (minutes_remaining / 60))
    c = 0.2 * fractional_game_left * s
    l = (.35 + .65 * (1 - fractional_game_left)) * s
    u = .45 * fractional_game_left * s
    d = max(c + l + u, current_score)
    f = max(original_projection, current_score)
    return f + (1 - fractional_game_left) * (d - f)


def _score_stats(scoring_settings, stats):
    return sum([
        v * scoring_settings[k] for (k, v) in stats.items()
        if k in scoring_settings
    ])


def _get_games_by_id(week):
    game_data = _graphql(_get_gamedata_query(week))
    games_by_id = {}
    for game in game_data['data']['scores']:
        if game['status'] == 'complete':
            seconds_left = 0
        elif game['status'] == 'pre_game':
            seconds_left = 3600
        else:
            quarter = game['metadata']['quarter_num']
            quarters_left = max(4 - quarter, 0)  # OT can go to 5
            qtr_time_remaining = game['metadata']['time_remaining'].split(':')
            qtr_mins_remaining = int(qtr_time_remaining[0])
            qtr_secs_remaining = int(qtr_time_remaining[1])
            seconds_left = (quarters_left * 15 * 60) + (
                qtr_mins_remaining * 60) + qtr_secs_remaining
        games_by_id[game['game_id']] = {
            'status': game['status'],
            'seconds_left': seconds_left
        }
    return games_by_id


def _update_espn(ds_client, bucket):
    for league_id in ESPN_LEAGUES.split(','):
        now = int(datetime.datetime.now().timestamp())
        league = League(int(league_id), 2021, espn_s2=ESPN_S2, swid=ESPN_SWID)

        week_key = ds_client.key('leagues', league_id, 'seasons', '2021',
                                 'weeks', str(league.current_week))
        entity = ds_client.get(week_key)
        if entity is None:
            entity = datastore.Entity(week_key)
            entity['projections'] = []

        matchup_id_by_team = {}
        for m in league.scoreboard():
            matchup_id_by_team[m.away_team.team_id] = m.data['id']
            matchup_id_by_team[m.home_team.team_id] = m.data['id']

        for m in league.box_scores():
            entity['projections'].extend([{
                'timestamp':
                now,
                'team_id':
                m.home_team.team_id,
                'matchup_id':
                matchup_id_by_team[m.home_team.team_id],
                'projection':
                m.home_projected
            }, {
                'timestamp':
                now,
                'team_id':
                m.away_team.team_id,
                'matchup_id':
                matchup_id_by_team[m.away_team.team_id],
                'projection':
                m.away_projected
            }])

        ds_client.put(entity)

        _write_html(bucket, league_id, league.current_week,
                    {t.team_id: t.team_name
                     for t in league.teams}, entity['projections'], 'espn')


def _write_html(bucket, league_id, week, team_id_to_name, projections,
                league_type):
    logging.info(f'generating html for league {league_id}')
    matchup_ids = set([p['matchup_id'] for p in projections])
    matchup_projections = {mid: [] for mid in matchup_ids}
    for p in projections:
        matchup_projections[p['matchup_id']].append(p)

    temp_dir = tempfile.TemporaryDirectory()

    team_id_key = 'roster_id' if league_type == 'sleeper' else 'team_id'
    matchup_team_ids = {}
    for m in matchup_ids:
        logging.info(f'generating html for matchup {m} in league {league_id}')
        this_matchup = matchup_projections[m]
        team_ids = list(set(p[team_id_key] for p in this_matchup))
        matchup_team_ids[m] = team_ids
        if len(team_ids) != 2:
            raise ValueError('wtf is happening here')

        team1_data = [p for p in this_matchup if team_ids[0] == p[team_id_key]]
        team2_data = [p for p in this_matchup if team_ids[1] == p[team_id_key]]
        plot = go.Figure([
            go.Scatter(x=[
                datetime.datetime.fromtimestamp(p['timestamp'])
                for p in team1_data
            ],
                       y=[p['projection'] for p in team1_data],
                       name=team_id_to_name[team_ids[0]]),
            go.Scatter(x=[
                datetime.datetime.fromtimestamp(p['timestamp'])
                for p in team2_data
            ],
                       y=[p['projection'] for p in team2_data],
                       name=team_id_to_name[team_ids[1]]),
        ])

        blob_pieces = [str(league_id), '2021', str(week), f'{m}.html']
        local_path = os.path.join(*([temp_dir.name] + blob_pieces))

        if not os.path.exists(os.path.dirname(local_path)):
            os.makedirs(os.path.dirname(local_path))

        with open(local_path, 'w') as f:
            plot.write_html(f, include_plotlyjs='cdn', auto_open=False)

        blob_name = f'{league_id}/2021/{week}/{m}.html'
        logging.info(f'writing {blob_name}')
        blob = bucket.blob(blob_name)
        blob.upload_from_filename(local_path)
        blob.make_public()

    _write_index(bucket, league_id, week, matchup_team_ids, team_id_to_name)


def _write_index(bucket, league_id, week, matchup_team_ids, team_id_to_name):
    blob = bucket.blob(f'{league_id}/2021/{week}/index.html')
    if blob.exists():
        logging.info('not generating index file')
        return

    tmpl = Template(filename='week_template.html')
    matchups = [{
        'name':
        f'{team_id_to_name[teams[0]]} vs. {team_id_to_name[teams[1]]}',
        'url':
        f'https://storage.googleapis.com/{PROJECTIONS_BUCKET}/{league_id}/2021/{week}/{mid}.html',
    } for (mid, teams) in matchup_team_ids.items()]
    blob.upload_from_string(tmpl.render(matchups=matchups, week=week),
                            content_type='text/html')
    blob.make_public()


if __name__ == '__main__':
    update_projections(None, None)
