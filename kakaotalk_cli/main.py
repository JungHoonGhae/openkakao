"""KakaoTalk CLI - Main entry point."""

import asyncio
import json
import sys
from datetime import datetime
from pathlib import Path

import click
from rich.console import Console
from rich.table import Table
from rich.panel import Panel

from .auth import get_credentials, get_credentials_interactive, KakaoCredentials
from .rest_client import KakaoRestClient

console = Console()

TOKEN_FILE = Path.home() / ".config" / "kakaotalk-cli" / "credentials.json"


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


@click.group()
@click.version_option(version="0.1.0")
def cli():
    """KakaoTalk CLI - Unofficial command-line client for KakaoTalk."""
    pass


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
            console.print("[yellow]No saved credentials. Run 'katalk login' first.[/yellow]")
            return

    console.print(f"  User ID: [bold]{creds.user_id}[/bold]")
    console.print(f"  Token:   {creds.oauth_token[:40]}...")
    console.print(f"  Version: {creds.app_version}")

    # Verify with REST API
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


@cli.command("friends")
@click.option("--favorites", "-f", is_flag=True, help="Show favorites only")
@click.option("--search", "-s", default=None, help="Search by name")
def friends(favorites: bool, search: str | None):
    """List friends."""
    client = _get_rest_client()
    friend_list = client.get_friends()

    if favorites:
        friend_list = [f for f in friend_list if f.favorite]

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


if __name__ == "__main__":
    cli()
