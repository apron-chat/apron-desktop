"""Bounded forward log scans with a fixed horizon and inclusive numeric cursors."""
from copy import deepcopy
from .model import Store, PAGE_SIZE, number

MAX_SCAN_PAGES = 128


def scan(session, rid, start, horizon, apply, done, valid=lambda: True):
    """One request at a time. Never claim completion after a gap or exhausted budget."""
    generation = session.generation
    count = 0
    def step(position):
        nonlocal count
        if generation != session.generation or not valid(): return
        if position > horizon: done(None); return
        if count >= MAX_SCAN_PAGES:
            done({'code': -1, 'message': 'History recovery reached its safety limit. This page cannot be verified; use latest or a narrower history range.'}); return
        count += 1
        def received(result, error):
            if generation != session.generation or not valid(): return
            if error: done(error); return
            try:
                latest = number(result['latest_log_id'])
                lower = number(result['history_log_id']) if result.get('history_log_id') is not None else latest + 1
                if lower > position: raise ValueError('History retention advanced during recovery. Reload latest.')
                apply(result)
                if result.get('more'):
                    last = number(result['last_log_id'])
                    if not position <= last <= horizon: raise ValueError('History did not advance within the requested bounds.')
                    step(last + 1)
                else: done(None)
            except (ValueError, KeyError, TypeError) as e:
                done({'code': -1, 'message': str(e) or 'Invalid history recovery response.'})
        session.request('history', {'room_id': rid, 'after': str(position), 'before': str(horizon), 'limit': PAGE_SIZE}, received)
    step(start)


def reconcile_page(session, rid, page, done, valid=lambda: True):
    """Bring only this page's messages/reactions forward to its frozen horizon."""
    try:
        horizon = number(page['latest_log_id'])
        start = number(page.get('last_log_id', page['latest_log_id'])) + 1
        selected = {m['message_id'] for m in page.get('messages', [])}
        projection = Store()
        projection.history(rid, page)
    except (ValueError, KeyError, TypeError) as e:
        done(None, {'code': -1, 'message': 'Invalid history page.'}); return
    def apply(result):
        lower = number(result['history_log_id']) if result.get('history_log_id') is not None else number(result['latest_log_id']) + 1
        if lower > number(page.get('first_log_id', str(start))):
            raise ValueError('History retention invalidated this page. Reload latest.')
        for msg in result.get('messages', []):
            if msg.get('message_id') in selected: projection.message(msg)
        for reaction in result.get('reactions', []):
            if reaction.get('message_id') in selected: projection.reaction(reaction)
    def complete(error):
        if error: done(None, error); return
        result = deepcopy(page)
        result['messages'] = [m for mid, m in projection.messages.items() if mid in selected]
        # Apply final per-user reaction snapshots without flattening their log IDs.
        result['reactions'] = [{'message_id': mid, 'log_id': str(log), 'reactions': [{'from': {'user_id': uid}, 'emojis': emojis}]} for (mid, uid), (log, emojis) in projection.reactions.items()]
        done(result, None)
    scan(session, rid, start, horizon, apply, complete, valid)
