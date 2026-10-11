#!/usr/bin/env python3
"""Seed only the dedicated loopback Go test server; never a supplied URL."""
import asyncio
import json
import websockets


async def main():
    async with websockets.connect('ws://127.0.0.1:18766/ws') as ws:
        await ws.recv()
        serial = 0
        async def request(method, params):
            nonlocal serial
            serial += 1; rid = str(serial)
            await ws.send(json.dumps({'method': method, 'params': params, 'id': rid}))
            while True:
                frame = json.loads(await ws.recv())
                if frame.get('id') == rid:
                    if 'error' in frame: raise RuntimeError(frame['error'])
                    return frame['result']
        await request('auth', {'scheme': 'guest', 'name': 'Synthetic benchmark author'})
        room = await request('room_set', {'title': 'Synthetic history benchmark'})
        text = 'Synthetic benchmark text. ' * 40
        for i in range(1000):
            await request('message', {'room_id': room['room_id'], 'body': {'text': f'{i:04d} {text}'}})
        print(json.dumps({'room_id': room['room_id'], 'messages': 1000, 'text_bytes_each': len(text.encode()) + 5}))


if __name__ == '__main__': asyncio.run(main())
