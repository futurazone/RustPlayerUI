#!/usr/bin/env python3
"""
Corrige metadatos de audio parseando la estructura de carpetas.

Los tags se leen de la ruta del archivo cuando el artista es "Unknown Artist".
Soporta MP3, M4A y FLAC vía mutagen.

Uso:
  python3 fix_metadata.py                        # Ejecutar (modifica archivos)
  python3 fix_metadata.py --dry-run              # Solo mostrar sin modificar
  python3 fix_metadata.py --music-dir /ruta       # Directorio custom
"""

import os
import re
import sys

MUSIC_DIR = "/MUSIC"
COVER_NAMES = {".album_cover.jpg", ".album_thumb.jpg", "cover.jpg",
               "folder.jpg", "front.jpg", "album.jpg"}

# Pattern: optional album prefix, then NN-NN Title.ext or NN Title.ext
FILE_PAT = re.compile(
    r'^(?:\d+-?\d+ - )?(?:.+? - .+? - )?(?:\d+-)?(\d+)\s+(.+)\.\w+$'
)
# Folder pattern: Artist - Album
FOLDER_PAT = re.compile(r'^(.+?) - (.+)$')


def fix_metadata(music_dir: str, dry_run: bool):
    stats = {"ok": 0, "skip": 0, "error": 0, "parsed": 0}
    missing_cover: list[tuple[str, str, str]] = []

    for root, dirs, files in os.walk(music_dir):
        # Saltar carpetas ocultas (._*, .DS_Store, etc.)
        dirs[:] = [d for d in dirs if not d.startswith('.')]

        folder = os.path.basename(root)
        fm = FOLDER_PAT.match(folder)
        if not fm:
            continue
        folder_artist, folder_album = fm.group(1).strip(), fm.group(2).strip()

        # Check cover presence
        has_cover = any(f in COVER_NAMES for f in files)
        if not has_cover:
            missing_cover.append((folder_artist, folder_album, root))

        for fname in files:
            ext = os.path.splitext(fname)[1].lower()
            if ext not in ('.mp3', '.m4a', '.flac'):
                continue

            path = os.path.join(root, fname)

            # Parse filename (needed for track number)
            fm2 = FILE_PAT.match(fname)
            if not fm2:
                stats["skip"] += 1
                continue

            track_str, raw_title = fm2.groups()
            track = int(track_str)
            title = raw_title.strip().rstrip('.')

            # Read current artist
            try:
                artist = _read_artist(path, ext)
            except Exception as e:
                print(f"  ERROR reading {fname}: {e}")
                stats["error"] += 1
                continue

            if artist and artist not in ("Unknown Artist", "Unknown"):
                current_track = _read_track(path, ext)
                if current_track == track:
                    stats["skip"] += 1
                    continue

            print(f"  {fname}")
            print(f"    Artist: {folder_artist}  Album: {folder_album}  "
                  f"Track: {track}  Title: {title}")

            if dry_run:
                stats["parsed"] += 1
                continue

            try:
                _write_tags(path, ext, title, folder_artist, folder_album, track)
                stats["ok"] += 1
            except Exception as e:
                print(f"    ERROR: {e}")
                stats["error"] += 1

    if missing_cover:
        print()
        print("=== CARPETAS SIN PORTADA ===")
        for artist, album, path in sorted(missing_cover, key=lambda x: x[0].lower()):
            print(f"  {artist} — {album}")
            print(f"    {path}")

    print()
    print(f"Total: {stats['ok']} corregidos, {stats['skip']} ya válidos, "
          f"{stats['parsed']} parseados (dry-run), {stats['error']} errores")


def _read_artist(path: str, ext: str) -> str:
    if ext == '.mp3':
        from mutagen.easyid3 import EasyID3
        audio = EasyID3(path)
        return audio.get('artist', [None])[0]
    elif ext == '.m4a':
        from mutagen.mp4 import MP4
        audio = MP4(path)
        return audio.get('\xa9ART', None)
    elif ext == '.flac':
        from mutagen.flac import FLAC
        audio = FLAC(path)
        return audio.get('artist', [None])[0]
    return None


def _read_track(path: str, ext: str) -> int:
    if ext == '.mp3':
        from mutagen.easyid3 import EasyID3
        audio = EasyID3(path)
        raw = audio.get('tracknumber', ['0'])[0]
        return int(raw.split('/')[0])
    elif ext == '.m4a':
        from mutagen.mp4 import MP4
        audio = MP4(path)
        return audio.get('trkn', [(0, 0)])[0][0]
    elif ext == '.flac':
        from mutagen.flac import FLAC
        audio = FLAC(path)
        raw = audio.get('tracknumber', ['0'])[0]
        return int(raw.split('/')[0])
    return 0


def _write_tags(path: str, ext: str, title: str, artist: str, album: str,
                track: int):
    if ext == '.mp3':
        from mutagen.easyid3 import EasyID3
        audio = EasyID3(path)
        audio['title'] = title
        audio['artist'] = artist
        audio['album'] = album
        audio['tracknumber'] = str(track)
        audio.save()
    elif ext == '.m4a':
        from mutagen.mp4 import MP4
        audio = MP4(path)
        audio['\xa9nam'] = title
        audio['\xa9ART'] = artist
        audio['\xa9alb'] = album
        audio['trkn'] = [(track, 0)]
        audio.save()
    elif ext == '.flac':
        from mutagen.flac import FLAC
        audio = FLAC(path)
        audio['title'] = title
        audio['artist'] = artist
        audio['album'] = album
        audio['tracknumber'] = str(track)
        audio.save()


if __name__ == '__main__':
    dry_run = '--dry-run' in sys.argv
    music_dir = MUSIC_DIR
    for i, arg in enumerate(sys.argv):
        if arg == '--music-dir' and i + 1 < len(sys.argv):
            music_dir = sys.argv[i + 1]

    if not os.path.isdir(music_dir):
        print(f"ERROR: '{music_dir}' no existe o no es un directorio")
        sys.exit(1)

    print(f"{'DRY RUN' if dry_run else 'FIX'} — Escaneando {music_dir}")
    fix_metadata(music_dir, dry_run)
