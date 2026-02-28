"""OpenKakao CLI - Main entry point."""

import json
import sys
from datetime import datetime
from pathlib import Path

import click
from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich.text import Text

from .auth import get_credentials, get_credentials_interactive, KakaoCredentials
from .rest_client import KakaoRestClient

console = Console()

TOKEN_FILE = Path.home() / ".config" / "openkakao" / "credentials.json"

_TYPE_LABELS = {
    "DirectChat": "DM",
    "MultiChat": "Group",
    "MemoChat": "Memo",
    "OpenDirectChat": "OpenDM",
    "OpenMultiChat": "OpenGroup",
}


def _save_credentials(creds: KakaoCredentials):
    TOKEN_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(TOKEN_FILE, "w") as f:
        json.dump({
            "oauth_token": creds.oauth_token,
            "user_id": creds.user_id,
            "device_uuid": creds.device_uuid,
            "device_name": creds.device_name,
            "app_version": creds.app_version,
            "user_agent": creds.user_agent,
            "a_header": creds.a_header,
        }, f, indent=2)
    TOKEN_FILE.chmod(0o600)
    console.print(f"[green]Credentials saved to {TOKEN_FILE}[/green]")


def _load_credentials() -> KakaoCredentials | None:
    if TOKEN_FILE.exists():
        with open(TOKEN_FILE) as f:
            data = json.load(f)
        return KakaoCredentials(**data)
    return None


def _get_creds() -> KakaoCredentials:
    """Get credentials from saved file or auto-extraction."""
    creds = _load_credentials()
    if creds:
        return creds

    creds = get_credentials()
    if creds:
        return creds

    creds = get_credentials_interactive()
    return creds


def _get_rest_client() -> KakaoRestClient:
    return KakaoRestClient(_get_creds())


def _format_time(epoch: int) -> str:
    if not epoch:
        return ""
    dt = datetime.fromtimestamp(epoch)
    now = datetime.now()
    if dt.date() == now.date():
        return dt.strftime("%H:%M")
    if dt.year == now.year:
        return dt.strftime("%m/%d %H:%M")
    return dt.strftime("%Y/%m/%d")


@click.group()
@click.version_option(version="0.2.0")
def cli():
    """OpenKakao - Unofficial command-line client for KakaoTalk."""
    pass


# ── Auth ─────────────────────────────────────────────────────────────

@cli.command()
def auth():
    """Check authentication status and verify token."""
    creds = get_credentials()
    if not creds:
        console.print("[red]Could not auto-extract token from KakaoTalk desktop app.[/red]")
        saved = _load_credentials()
        if saved:
            console.print(f"[green]Saved credentials found (user: {saved.user_id})[/green]")
            creds = saved
        else:
            console.print("[yellow]No saved credentials. Run 'openkakao login' first.[/yellow]")
            return

    console.print(f"  User ID: [bold]{creds.user_id}[/bold]")
    console.print(f"  Token:   {creds.oauth_token[:40]}...")
    console.print(f"  Version: {creds.app_version}")

    client = KakaoRestClient(creds)
    if client.verify_token():
        console.print("[green]  Token is valid![/green]")
    else:
        console.print("[red]  Token is invalid or expired.[/red]")


@cli.command("login")
@click.option("--save", is_flag=True, help="Save extracted credentials")
def login(save: bool):
    """Extract and optionally save credentials from KakaoTalk desktop app."""
    creds = get_credentials()
    if not creds:
        console.print("[red]Could not extract credentials. Is KakaoTalk running?[/red]")
        return

    console.print(f"[green]Credentials extracted![/green]")
    console.print(f"  User ID: [bold]{creds.user_id}[/bold]")
    console.print(f"  Token:   {creds.oauth_token[:40]}...")

    client = KakaoRestClient(creds)
    if client.verify_token():
        console.print("[green]  Token verified OK[/green]")
    else:
        console.print("[yellow]  Token may be expired for some operations[/yellow]")

    if save:
        _save_credentials(creds)


# ── Profile ──────────────────────────────────────────────────────────

@cli.command("me")
def my_profile():
    """Show my profile information."""
    client = _get_rest_client()
    profile = client.get_my_profile()

    console.print(Panel(f"[bold]{profile.nickname}[/bold]", title="My Profile"))
    if profile.status_message:
        console.print(f"  Status:  {profile.status_message}")
    console.print(f"  Email:   {profile.email}")
    console.print(f"  Account: {profile.account_id}")
    console.print(f"  User ID: {profile.user_id}")
    if profile.profile_image_url:
        console.print(f"  Image:   {profile.profile_image_url}")


# ── Friends ──────────────────────────────────────────────────────────

@cli.command("friends")
@click.option("--favorites", "-f", is_flag=True, help="Show favorites only")
@click.option("--hidden", is_flag=True, help="Show hidden friends")
@click.option("--search", "-s", default=None, help="Search by name")
def friends(favorites: bool, hidden: bool, search: str | None):
    """List friends."""
    client = _get_rest_client()
    friend_list = client.get_friends()

    if favorites:
        friend_list = [f for f in friend_list if f.favorite]

    if not hidden:
        friend_list = [f for f in friend_list if not f.hidden]

    if search:
        q = search.lower()
        friend_list = [f for f in friend_list
                       if q in f.display_name.lower() or q in f.phone_number]

    table = Table(title=f"Friends ({len(friend_list)})")
    table.add_column("Name", style="cyan")
    table.add_column("Status", style="dim")
    table.add_column("Phone", style="dim")
    table.add_column("User ID", style="dim", justify="right")

    for f in friend_list:
        fav = " *" if f.favorite else ""
        name = f.display_name + fav
        table.add_row(name, f.status_message[:30], f.phone_number, str(f.user_id))

    console.print(table)


# ── Chat Rooms ───────────────────────────────────────────────────────

@cli.command("chats")
@click.option("--all", "-a", "show_all", is_flag=True, help="Show all chats (paginate)")
@click.option("--unread", "-u", is_flag=True, help="Show only unread chats")
def chats(show_all: bool, unread: bool):
    """List chat rooms."""
    client = _get_rest_client()

    if show_all:
        chat_list = client.get_all_chats()
    else:
        chat_list, _ = client.get_chats()

    if unread:
        chat_list = [c for c in chat_list if c.unread_count > 0]

    table = Table(title=f"Chats ({len(chat_list)})")
    table.add_column("Type", style="dim", width=6)
    table.add_column("Name", style="cyan")
    table.add_column("Unread", justify="right")
    table.add_column("Chat ID", style="dim", justify="right")

    for c in chat_list:
        type_label = _TYPE_LABELS.get(c.type, c.type[:6])
        unread_str = str(c.unread_count) if c.unread_count > 0 else ""
        unread_style = "bold red" if c.unread_count > 0 else ""
        table.add_row(
            type_label,
            c.display_title,
            Text(unread_str, style=unread_style),
            str(c.chat_id),
        )

    console.print(table)


@cli.command("read")
@click.argument("chat_id", type=int)
@click.option("--count", "-n", default=30, help="Number of messages to show")
@click.option("--all", "-a", "fetch_all", is_flag=True, help="Fetch all available messages (cursor pagination)")
def read_chat(chat_id: int, count: int, fetch_all: bool):
    """Read messages from a chat room."""
    creds = _get_creds()
    client = KakaoRestClient(creds)

    if fetch_all:
        messages = client.get_all_messages(chat_id)
    else:
        messages, _ = client.get_messages(chat_id)
        # Show in chronological order (oldest first), limited to count
        messages = list(reversed(messages[:count]))

    # Get members for name resolution
    try:
        members = client.get_chat_members(chat_id)
        member_map = {m.user_id: m.display_name for m in members}
    except Exception:
        member_map = {}

    member_map[creds.user_id] = "Me"

    if not messages:
        console.print("[dim]No messages (server cache may be empty for this chat).[/dim]")
        return

    for msg in messages:
        name = member_map.get(msg.author_id, str(msg.author_id))
        time_str = _format_time(msg.send_at)

        if msg.author_id == creds.user_id:
            name_style = "bold green"
        else:
            name_style = "bold cyan"

        if msg.type == 1:
            console.print(f"[dim]{time_str}[/dim] [{name_style}]{name}[/{name_style}]: {msg.message}")
        elif msg.type == 2:
            console.print(f"[dim]{time_str}[/dim] [{name_style}]{name}[/{name_style}]: [dim](photo)[/dim]")
        elif msg.type == 71:
            console.print(f"[dim]{time_str}[/dim] [{name_style}]{name}[/{name_style}]: [dim](emoticon)[/dim]")
        else:
            text = msg.message or f"(type={msg.type})"
            console.print(f"[dim]{time_str}[/dim] [{name_style}]{name}[/{name_style}]: {text}")

    console.print(f"\n[dim]Showing {len(messages)} messages.[/dim]")


@cli.command("members")
@click.argument("chat_id", type=int)
def chat_members(chat_id: int):
    """Show members of a chat room."""
    client = _get_rest_client()
    members = client.get_chat_members(chat_id)

    table = Table(title=f"Members ({len(members)})")
    table.add_column("Name", style="cyan")
    table.add_column("User ID", style="dim", justify="right")
    table.add_column("Country", style="dim")

    for m in members:
        table.add_row(m.display_name, str(m.user_id), m.country_iso)

    console.print(table)


# ── Settings ─────────────────────────────────────────────────────────

@cli.command("settings")
def show_settings():
    """Show account settings."""
    client = _get_rest_client()
    settings = client.get_settings()

    console.print(Panel("[bold]Account Settings[/bold]"))
    console.print(f"  Status:    {settings.get('status')}")
    console.print(f"  Account:   {settings.get('accountId')}")
    console.print(f"  Email:     {settings.get('emailAddress')}")
    console.print(f"  Country:   {settings.get('countryIso')}")
    console.print(f"  Version:   {settings.get('recentVersion')}")
    console.print(f"  Server:    {settings.get('server_time')}")

    profile = settings.get("profile", {})
    if profile:
        console.print(f"\n  Nickname:  {profile.get('nickname')}")
        console.print(f"  Status:    {profile.get('statusMessage')}")


# ── Utility ──────────────────────────────────────────────────────────

@cli.command("scrap")
@click.argument("url")
def scrap_preview(url: str):
    """Get link preview for a URL."""
    client = _get_rest_client()
    data = client.get_scrap_preview(url)

    if data.get("status") != 0:
        console.print(f"[red]Error: {data.get('status')}[/red]")
        return

    console.print(Panel(f"[bold]{data.get('title', '')}[/bold]", title="Link Preview"))
    if data.get("description"):
        console.print(f"  {data['description'][:200]}")
    console.print(f"  URL: {data.get('canonicalUrl', url)}")
    if data.get("mainImageUrl"):
        console.print(f"  Image: {data['mainImageUrl']}")


if __name__ == "__main__":
    cli()
