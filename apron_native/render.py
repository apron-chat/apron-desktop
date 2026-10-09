"""CommonMark to a restricted Pango subset. No HTML or remote resources."""
from html import escape
from markdown_it import MarkdownIt
from .model import safe_text

_parser = MarkdownIt('commonmark', {'html': False, 'maxNesting': 20})


def markdown(text):
    out = []
    for token in _parser.parse(safe_text(text)):
        if token.type == 'inline':
            for t in token.children or []:
                if t.type in ('text', 'html_inline'):
                    out.append(escape(t.content))
                elif t.type in ('softbreak', 'hardbreak'):
                    out.append('\n')
                elif t.type == 'code_inline':
                    out.append('<tt>' + escape(t.content) + '</tt>')
                elif t.type in ('strong_open', 'strong_close', 'em_open', 'em_close'):
                    tag = 'b' if t.type.startswith('strong') else 'i'
                    out.append(f'<{tag}>' if t.type.endswith('open') else f'</{tag}>')
                elif t.type == 'image':
                    out.append('[Image: ' + escape(t.content) + ']')
                elif t.type == 'link_close':
                    # Link text renders without executing or fetching its destination.
                    pass
        elif token.type in ('fence', 'code_block'):
            out.append('<tt>' + escape(token.content.rstrip()) + '</tt>\n\n')
        elif token.type == 'heading_open':
            out.append('<b>')
        elif token.type == 'heading_close':
            out.append('</b>\n\n')
        elif token.type == 'paragraph_close':
            out.append('\n' if token.hidden else '\n\n')
        elif token.type == 'list_item_open':
            out.append('• ')
        elif token.type == 'blockquote_open':
            out.append('│ ')
        elif token.type == 'hr':
            out.append('────────\n')
    return ''.join(out).strip()


def embed_text(embed):
    og = embed.get('og') or {}
    return safe_text(' · '.join(str(x) for x in [
        embed.get('kind', 'Attachment'), og.get('title') or embed.get('title'),
        og.get('description') or embed.get('text'), embed.get('url')
    ] if x), 1000)
