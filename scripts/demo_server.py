#!/usr/bin/env python3
"""Loopback-only, in-memory protocol-8 fixture. Entirely fictional content."""
import asyncio
import json
import time
import websockets

BASE = 1791536400000
ADA = {'user_id': 'ada', 'name': 'Ada'}
NOOR = {'user_id': 'noor', 'name': 'Noor'}
YOU = {'user_id': 'demo_you', 'name': 'You'}
MEMBERS = [ADA, NOOR, YOU]
ROOMS = {
    'general': {'room_id': 'general', 'title': 'General', 'description': 'Good conversations start here.'},
    'design': {'room_id': 'design', 'title': 'Design', 'description': 'Thoughtful details, shared openly.'},
    'engineering': {'room_id': 'engineering', 'title': 'Engineering', 'description': 'Small tools. Useful things.'},
    'weekend': {'room_id': 'weekend', 'title': 'Weekend', 'description': 'A little room for everything else.'},
    'native': {'room_id': 'native', 'parent_room_id': 'general', 'title': 'A native home for Apron', 'description': 'Linux, Wayland, and the details that matter.'},
}
for i, room in enumerate(ROOMS.values()):
    room.update(log_id=str(BASE + i), history_log_id=str(BASE), latest_log_id=str(BASE + 300), members=MEMBERS)
MESSAGES = []
for i in range(1, 76):
    MESSAGES.append({'message_id': str(BASE + 1000 + i), 'log_id': str(BASE + 1000 + i), 'room_id': 'general',
                     'from': ADA if i % 2 else NOOR, 'body': {'text': f'Local fixture note {i:02}: making space for focused conversations.', 'format': 'plain'}})
examples = [
    (ADA, 'A small place for big ideas.\nWelcome to the local Apron demo — everything here is fictional.', []),
    (NOOR, 'The native client is coming together: a calmer message view, a quiet sidebar, and a composer that stays out of the way.', []),
    (ADA, 'A few details to try:\n- Find a room with Ctrl+K\n- Right-click a message to reply\n- Send with Ctrl+Enter\n\nNo accounts or external services needed.', []),
    (YOU, 'Feels right at home on Wayland. The keyboard shortcuts are a nice touch.', []),
    (NOOR, '@demo_you what do you think of the thread layout? I left a space for the discussion below.', ['demo_you']),
]
for i, (user, text, mentions) in enumerate(examples):
    mid = str(BASE + 1080 + i)
    MESSAGES.append({'message_id': mid, 'log_id': mid, 'room_id': 'general', 'from': user,
                     'body': {'text': text, 'format': 'markdown', 'mentions': mentions}})
MESSAGES.append({'message_id': str(BASE + 1100), 'log_id': str(BASE + 1100), 'room_id': 'native', 'from': ADA,
                 'body': {'text': 'A thread is its own room, with its own history.\nTry a reply, or start another thread in General.'}})
CLIENTS = set()
COUNTER = BASE + 2000


def record(room):
    related = [int(m['log_id']) for m in MESSAGES if m['room_id'] == room['room_id']]
    return {**room, 'latest_log_id': str(max(related, default=int(room['log_id'])))}


async def handler(ws):
    global COUNTER
    joined = {'general', 'design', 'engineering', 'native'}
    authed = False
    CLIENTS.add(ws)
    async def send(frame):
        await ws.send(json.dumps(frame))
    try:
        await send({'method': 'server', 'params': {'apron': 8, 'auth': ['guest', 'token'],
                    'capabilities': ['rooms', 'history', 'activity', 'reactions'], 'ping': 20, 'agent': 'fictional-local-fixture/1'}})
        async for raw in ws:
            req = json.loads(raw)
            method, p, rid = req.get('method'), req.get('params', {}), req.get('id')
            result = {}
            if method == 'ping':
                await send({'method': 'pong'}); continue
            if method == 'auth':
                if p.get('scheme') == 'token' and p.get('token') != 'local-fixture-token':
                    await send({'id': rid, 'error': {'code': -32001, 'message': 'Invalid local fixture token'}}); continue
                authed = True
                result = {'you': YOU, 'token': 'local-fixture-token'}
            elif not authed:
                await send({'id': rid, 'error': {'code': -32001, 'message': 'Authenticate first'}}); continue
            elif method == 'room_list':
                selected = [r for r in ROOMS.values() if (r.get('parent_room_id') == p['parent_room_id'] if 'parent_room_id' in p else True)]
                result = {'joined': [record(r) for r in selected if r['room_id'] in joined],
                          'not_joined': [record(r) for r in selected if r['room_id'] not in joined], 'users': MEMBERS}
            elif method == 'history':
                room_id = p.get('room_id', 'general')
                messages = sorted([m for m in MESSAGES if m['room_id'] == room_id], key=lambda m: int(m['log_id']))
                selected = [m for m in messages if int(p.get('after', '0')) <= int(m['log_id']) <= int(p.get('before', str(10**30))) ]
                limit = min(50, max(1, int(p.get('limit', 50))))
                page = selected[:limit] if 'after' in p else selected[-limit:]
                result = {'messages': page, 'more': len(selected) > len(page), 'latest_log_id': record(ROOMS[room_id])['latest_log_id'], 'history_log_id': str(BASE)}
                if page:
                    result.update(first_log_id=page[0]['log_id'], last_log_id=page[-1]['log_id'])
            elif method == 'reactions':
                COUNTER = max(COUNTER + 1, int(time.time() * 1000))
                reaction_record = {'message_id': p['message_id'], 'log_id': str(COUNTER), 'room_id': 'general', 'reactions': [{'from': YOU, 'emojis': p.get('emojis', [])}]}
                for client in list(CLIENTS):
                    try: await client.send(json.dumps({'method': 'reactions', 'params': reaction_record}))
                    except websockets.ConnectionClosed: pass
                result = {}
            elif method == 'message':
                room_id = p.get('room_id', 'general')
                if p.get('body', {}).get('text') == '/test-disconnect':
                    await ws.close(); return
                if p.get('body', {}).get('text') == '/test-denied':
                    await send({'id': rid, 'error': {'code': -32001, 'message': 'Fictional permission error; draft retained.'}}); continue
                COUNTER = max(COUNTER + 1, int(time.time() * 1000))
                mid = str(COUNTER)
                msg = {**p, 'room_id': room_id, 'message_id': mid, 'log_id': mid, 'from': YOU}
                MESSAGES.append(msg)
                del MESSAGES[:-1000]
                for client in list(CLIENTS):
                    try:
                        await client.send(json.dumps({'method': 'message', 'params': msg}))
                    except websockets.ConnectionClosed:
                        pass
                result = {'message_id': mid}
            elif method == 'room_join':
                joined.add(p['room_id'])
                await send({'method': 'room_update', 'params': {'joined': [record(ROOMS[p['room_id']])]}})
            elif method == 'room_set' and p.get('room_id') in ROOMS:
                joined.add(p['room_id'])
                await send({'method': 'room_update', 'params': {'joined': [record(ROOMS[p['room_id']])]}})
                result = {'room_id': p['room_id']}
            elif method == 'room_set':
                COUNTER += 1
                new_id = str(COUNTER)
                room = {**p, 'room_id': new_id, 'log_id': new_id, 'history_log_id': new_id, 'latest_log_id': new_id, 'members': MEMBERS}
                ROOMS[new_id] = room; joined.add(new_id)
                await send({'method': 'room_update', 'params': {'joined': [room]}})
                result = {'room_id': new_id}
            elif method == 'activity':
                await send({'method': 'activity', 'params': {**p, 'from': YOU}}); continue
            else:
                await send({'id': rid, 'error': {'code': -32601, 'message': 'Unsupported in local fixture'}}); continue
            if rid:
                await send({'id': rid, 'result': result})
    finally:
        CLIENTS.discard(ws)


async def main():
    async with websockets.serve(handler, '127.0.0.1', 18765, max_size=256*1024):
        print('Fictional local Apron fixture: ws://127.0.0.1:18765', flush=True)
        await asyncio.Future()


if __name__ == '__main__':
    asyncio.run(main())
