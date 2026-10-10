"""Payload builders preserve client fields when protocol snapshots are saved."""
from copy import deepcopy


def message_save(message, *, text=None, room_id=None, deleted=False):
    if not message.get('message_id') or message.get('deleted'):
        raise ValueError('This message cannot be changed.')
    result = {k: deepcopy(message[k]) for k in ('message_id', 'room_id', 'body', 'ext') if k in message}
    if message.get('reply_to', {}).get('message_id'):
        result['reply_to'] = {'message_id': message['reply_to']['message_id']}
    if deleted:
        return {'message_id': message['message_id'], 'room_id': message['room_id'], 'deleted': True}
    if text is not None:
        if not text.strip() and not result.get('body', {}).get('embeds'):
            raise ValueError('Keep text or an attachment, or delete the message.')
        result.setdefault('body', {})['text'] = text
    if room_id is not None:
        result['room_id'] = room_id
    return result


def same_snapshot(store, message):
    current = store.messages.get(message['message_id'])
    return current is not None and current.get('log_id') == message.get('log_id') and not current.get('deleted')
