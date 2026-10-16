"""Card hubs and page metadata for the documentation site, registered as an MkDocs hook.

A page containing ``<!-- cards -->`` gets a card for every sibling in its nav section (or every
top-level section on the home page). ``<!-- cards grouped -->`` renders one heading per child
section followed by that section's cards. Cards come from front matter, so a new page or folder
appears without editing any list:

    title: Async / Await
    summary: Write non-blocking code with async functions and await.
    icon: material/timer-sand
    status: stable
    targets: [native, web, node]
    api: reference/api/system-task.md
    related: [language/concurrency/tasks.md]

Rendering happens in ``on_page_context`` because every page's metadata and title are known by
then; ``on_page_markdown`` runs before later pages have been read.
"""

import html
import logging
import re
from pathlib import Path

from mkdocs.structure.nav import Section
from mkdocs.structure.pages import Page
from mkdocs.utils import get_relative_url

log = logging.getLogger("mkdocs.hooks.site")

STATUSES = ("stable", "experimental", "planned")
TARGETS = ("native", "web", "node")
STATUS_LABELS = {"stable": "Stable", "experimental": "Experimental", "planned": "Planned"}
TARGET_LABELS = {"native": "Native", "web": "Browser", "node": "Node"}
VALIDATED_ROOTS = ("language/",)
WORDS_PER_MINUTE = 220
MARKER = re.compile(r"<!--\s*cards(\s+grouped)?\s*-->")
DEFAULT_ICONS = {"section": "material/folder-outline", "page": "material/file-document-outline"}

_state = {"nav": None, "files": None, "icons": None}


def _icon_root():
    import material

    return Path(material.__file__).parent / "templates" / ".icons"


def icon_svg(name, root=None):
    root = root or _state["icons"] or _icon_root()
    path = (root / f"{name}.svg").resolve()
    if root.resolve() not in path.parents or not path.is_file():
        return None
    return path.read_text(encoding="utf-8")


def inline_markup(text):
    escaped = html.escape(str(text), quote=False)
    return re.sub(r"`([^`]+)`", r"<code>\1</code>", escaped)


def index_page(section):
    return next((child for child in section.children if isinstance(child, Page) and child.is_index), None)


def page_count(section):
    count = 0
    for child in section.children:
        if isinstance(child, Section):
            count += page_count(child)
        elif isinstance(child, Page) and not child.is_index:
            count += 1
    return count


def card_items(page, nav_items):
    """Nav entries a hub page shows as cards: its section's other children, or top-level sections."""
    if page.is_homepage or page.parent is None:
        siblings = nav_items
    elif page.is_index:
        siblings = page.parent.children
    else:
        return None
    return [item for item in siblings if item is not page and _card_meta(item).get("card", True) is not False]


def _card_meta(item):
    if isinstance(item, Section):
        index = index_page(item)
        return dict(index.meta) if index else {}
    if isinstance(item, Page):
        return dict(item.meta)
    return {}


def validate_meta(src_uri, meta, *, is_index=False):
    """Problems with a page's front matter, as human-readable messages."""
    problems = []
    if not src_uri.startswith(VALIDATED_ROOTS):
        return problems
    if not meta.get("summary"):
        problems.append("missing `summary`")
    if is_index:
        if not meta.get("icon"):
            problems.append("missing `icon`")
        return problems
    status = meta.get("status")
    if status not in STATUSES:
        problems.append(f"`status` must be one of {', '.join(STATUSES)} (found {status!r})")
    targets = meta.get("targets")
    if not isinstance(targets, list) or not targets or any(target not in TARGETS for target in targets):
        problems.append(f"`targets` must be a non-empty list drawn from {', '.join(TARGETS)} (found {targets!r})")
    return problems


def badges(meta, *, compact=False):
    """Status and target badges. Compact (card) badges omit targets when a feature runs everywhere."""
    parts = []
    status = meta.get("status")
    if status in STATUSES and status != "stable":
        parts.append(f'<span class="dream-badge dream-badge--{status}">{STATUS_LABELS[status]}</span>')
    targets = [target for target in meta.get("targets") or [] if target in TARGETS]
    if compact and set(targets) == set(TARGETS):
        targets = []
    for target in targets:
        parts.append(f'<span class="dream-badge dream-badge--target">{TARGET_LABELS[target]} only</span>'
                     if compact and len(targets) == 1 else
                     f'<span class="dream-badge dream-badge--target">{TARGET_LABELS[target]}</span>')
    return "".join(parts)


def page_header(content, meta):
    """Insert the badge row and reading time under the page's first heading."""
    minutes = meta.get("reading_time")
    row = badges(meta) + (f'<span class="dream-reading-time">{minutes} min read</span>' if minutes else "")
    if not row or "</h1>" not in content:
        return content
    return content.replace("</h1>", f'</h1><div class="dream-page-meta">{row}</div>', 1)


def _url_for(src_uri, files):
    file = files.get_file_from_path(src_uri) if files is not None else None
    return file.url if file is not None else None


def render_card(item, page, files=None, icon_root=None):
    meta = _card_meta(item)
    if isinstance(item, Section):
        target = index_page(item)
        if target is None:
            log.warning("Section '%s' has no index.md, so it cannot be shown as a card on %s",
                        item.title, page.file.src_uri)
            return ""
        title = item.title
        kind = "section"
        count = page_count(item)
        footer = f'<span class="dream-card__count">{count} {"page" if count == 1 else "pages"}</span>' if count else ""
    else:
        target = item
        title = item.title
        kind = "page"
        footer = ""
    summary = meta.get("summary")
    if not summary:
        log.warning("%s is shown as a card on %s but has no `summary` front matter",
                    target.file.src_uri, page.file.src_uri)
    icon = icon_svg(meta.get("icon") or DEFAULT_ICONS[kind], icon_root)
    if icon is None:
        log.warning("Unknown icon %r in %s", meta.get("icon"), target.file.src_uri)
        icon = icon_svg(DEFAULT_ICONS[kind], icon_root) or ""
    href = get_relative_url(target.url, page.url)
    api = ""
    if meta.get("api"):
        api_url = _url_for(meta["api"], files)
        if api_url is None:
            log.warning("`api` in %s points to a missing page: %s", target.file.src_uri, meta["api"])
        else:
            api = f'<a class="dream-card__api" href="{html.escape(get_relative_url(api_url, page.url))}">API</a>'
    status = meta.get("status") if meta.get("status") in STATUSES else ""
    targets = " ".join(t for t in meta.get("targets") or [] if t in TARGETS)
    tail = badges(meta, compact=True) + footer + api
    return (
        f'<div class="dream-card dream-card--{kind}" data-status="{status}" data-targets="{targets}">'
        f'<span class="dream-card__icon">{icon}</span>'
        f'<a class="dream-card__title" href="{html.escape(href)}">{inline_markup(title)}</a>'
        f'<p class="dream-card__summary">{inline_markup(summary or "")}</p>'
        + (f'<div class="dream-card__meta">{tail}</div>' if tail else "")
        + "</div>"
    )


def _slug(title):
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")


def render_hub(page, items, grouped, files=None, icon_root=None):
    def grid(entries):
        cards = "".join(render_card(entry, page, files, icon_root) for entry in entries)
        return f'<div class="dream-cards">{cards}</div>' if cards else ""

    if not grouped:
        return grid(items)
    leaves = [item for item in items if not isinstance(item, Section)]
    parts = [grid(leaves)] if leaves else []
    for section in (item for item in items if isinstance(item, Section)):
        index = index_page(section)
        if index is None:
            log.warning("Section '%s' has no index.md, so it cannot be grouped on %s",
                        section.title, page.file.src_uri)
            continue
        children = [child for child in section.children
                    if child is not index and _card_meta(child).get("card", True) is not False]
        summary = index.meta.get("summary", "")
        href = html.escape(get_relative_url(index.url, page.url))
        icon = icon_svg(index.meta.get("icon") or DEFAULT_ICONS["section"], icon_root) or ""
        parts.append(
            f'<section class="dream-card-group" id="{_slug(section.title)}">'
            f'<div class="dream-card-group__header"><span class="dream-card__icon">{icon}</span>'
            f'<div><h2><a href="{href}">{inline_markup(section.title)}</a></h2>'
            f'<p>{inline_markup(summary)}</p></div></div>'
            f"{grid(children)}</section>"
        )
    return "".join(parts)


def related(page, files):
    entries = []
    for src_uri in page.meta.get("related") or []:
        file = files.get_file_from_path(src_uri) if files is not None else None
        if file is None or file.page is None:
            log.warning("`related` in %s points to a missing page: %s", page.file.src_uri, src_uri)
            continue
        target = file.page
        entries.append({"title": target.title, "url": get_relative_url(target.url, page.url),
                        "summary": target.meta.get("summary", "")})
    return entries


def on_config(config):
    _state["icons"] = _icon_root()
    return config


def on_files(files, config):
    _state["files"] = files
    return files


def on_nav(nav, config, files):
    _state["nav"] = nav
    return nav


def on_page_markdown(markdown, page, config, files):
    words = len(re.sub(r"```.*?```", " ", markdown, flags=re.S).split())
    page.meta.setdefault("reading_time", max(1, round(words / WORDS_PER_MINUTE)))
    return markdown


def on_page_context(context, page, config, nav):
    files = _state["files"]
    for problem in validate_meta(page.file.src_uri, page.meta, is_index=page.is_index):
        log.warning("%s: %s", page.file.src_uri, problem)
    context["dream_related"] = related(page, files)
    match = MARKER.search(page.content or "")
    if not match and "navigation" not in (page.meta.get("hide") or []):
        page.content = page_header(page.content or "", page.meta)
    if match:
        items = card_items(page, nav.items)
        if items is None:
            log.warning("%s uses a cards marker but is not a section index", page.file.src_uri)
        else:
            hub = render_hub(page, items, bool(match.group(1)), files)
            page.content = MARKER.sub(lambda _: hub, page.content, count=1)
    return context

