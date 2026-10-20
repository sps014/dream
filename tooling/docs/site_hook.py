"""Section directories and page metadata for the documentation site, registered as an MkDocs hook.

A page containing ``<!-- pages -->`` lists every sibling in its nav section. The home page shows
the top-level sections as cards; section pages show a directory of rows, so navigating deeper never
means another wall of cards. ``<!-- pages grouped -->`` renders one column per child section with
that section's pages. Entries come from front matter, so a new page or folder appears without
editing any list:

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
TARGETS = ("native", "web", "node", "wasip2", "wasip3")
STATUS_LABELS = {"stable": "Stable", "experimental": "Experimental", "planned": "Planned"}
TARGET_LABELS = {"native": "Native", "web": "Browser", "node": "Node", "wasip2": "WASI Preview 2", "wasip3": "WASI Preview 3"}
VALIDATED_ROOTS = ("language/",)
WORDS_PER_MINUTE = 220
MARKER = re.compile(r"<!--\s*pages(\s+grouped)?\s*-->")
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
    return _listable(item for item in siblings if item is not page)


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


def _entry(item, page, files):
    """Link target, title, and metadata for one hub entry, or None when it cannot be linked."""
    meta = _card_meta(item)
    target = index_page(item) if isinstance(item, Section) else item
    if target is None:
        log.warning("Section '%s' has no index.md, so it cannot be listed on %s", item.title, page.file.src_uri)
        return None
    if not meta.get("summary"):
        log.warning("%s is listed on %s but has no `summary` front matter", target.file.src_uri, page.file.src_uri)
    api = ""
    if meta.get("api"):
        api_url = _url_for(meta["api"], files)
        if api_url is None:
            log.warning("`api` in %s points to a missing page: %s", target.file.src_uri, meta["api"])
        else:
            api = get_relative_url(api_url, page.url)
    return {
        "kind": "section" if isinstance(item, Section) else "page",
        "title": item.title,
        "href": get_relative_url(target.url, page.url),
        "summary": meta.get("summary") or "",
        "meta": meta,
        "count": page_count(item) if isinstance(item, Section) else 0,
        "api": api,
        "src_uri": target.file.src_uri,
    }


def _filter_attrs(meta):
    status = meta.get("status") if meta.get("status") in STATUSES else ""
    targets = " ".join(t for t in meta.get("targets") or [] if t in TARGETS)
    return f'data-status="{status}" data-targets="{targets}"'


def _count_label(count):
    return f'{count} {"page" if count == 1 else "pages"}' if count else ""


def render_card(item, page, files=None, icon_root=None):
    entry = _entry(item, page, files)
    if entry is None:
        return ""
    icon = icon_svg(entry["meta"].get("icon") or DEFAULT_ICONS[entry["kind"]], icon_root)
    if icon is None:
        log.warning("Unknown icon %r in %s", entry["meta"].get("icon"), entry["src_uri"])
        icon = icon_svg(DEFAULT_ICONS[entry["kind"]], icon_root) or ""
    count = _count_label(entry["count"])
    return (
        f'<div class="dream-card">'
        f'<span class="dream-card__icon">{icon}</span>'
        f'<a class="dream-card__title" href="{html.escape(entry["href"])}">{inline_markup(entry["title"])}</a>'
        f'<p class="dream-card__summary">{inline_markup(entry["summary"])}</p>'
        + (f'<span class="dream-card__count">{count}</span>' if count else "")
        + "</div>"
    )


def render_row(item, page, files=None):
    entry = _entry(item, page, files)
    if entry is None:
        return ""
    tail = badges(entry["meta"], compact=True)
    if entry["count"]:
        tail += f'<span class="dream-index__count">{_count_label(entry["count"])}</span>'
    if entry["api"]:
        tail += f'<a class="dream-index__api" href="{html.escape(entry["api"])}">API</a>'
    return (
        f'<li class="dream-index__item" {_filter_attrs(entry["meta"])}>'
        f'<a class="dream-index__link" href="{html.escape(entry["href"])}">'
        f'<span class="dream-index__title">{inline_markup(entry["title"])}</span>'
        f'<span class="dream-index__summary">{inline_markup(entry["summary"])}</span></a>'
        + (f'<span class="dream-index__meta">{tail}</span>' if tail else "")
        + "</li>"
    )


def _slug(title):
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")


def _listable(items):
    return [item for item in items if _card_meta(item).get("listed", True) is not False]


def render_hub(page, items, grouped, files=None, icon_root=None):
    if page.is_homepage:
        cards = "".join(render_card(item, page, files, icon_root) for item in items)
        return f'<div class="dream-cards">{cards}</div>' if cards else ""

    def rows(entries):
        body = "".join(render_row(entry, page, files) for entry in entries)
        return f'<ul class="dream-index">{body}</ul>' if body else ""

    if not grouped:
        return rows(items)
    leaves = [item for item in items if not isinstance(item, Section)]
    parts = [rows(leaves)] if leaves else []
    groups = []
    for section in (item for item in items if isinstance(item, Section)):
        index = index_page(section)
        if index is None:
            log.warning("Section '%s' has no index.md, so it cannot be grouped on %s",
                        section.title, page.file.src_uri)
            continue
        children = _listable(child for child in section.children if child is not index)
        href = html.escape(get_relative_url(index.url, page.url))
        groups.append(
            f'<section class="dream-directory__group" id="{_slug(section.title)}">'
            f'<h2><a href="{href}">{inline_markup(section.title)}</a></h2>'
            f'<p>{inline_markup(index.meta.get("summary", ""))}</p>'
            f"{rows(children)}</section>"
        )
    if groups:
        parts.append(f'<div class="dream-directory">{"".join(groups)}</div>')
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
    if not match and not page.is_homepage:
        page.content = page_header(page.content or "", page.meta)
    if match:
        items = card_items(page, nav.items)
        if items is None:
            log.warning("%s uses a pages marker but is not a section index", page.file.src_uri)
        else:
            hub = render_hub(page, items, bool(match.group(1)), files)
            page.content = MARKER.sub(lambda _: hub, page.content, count=1)
    return context

