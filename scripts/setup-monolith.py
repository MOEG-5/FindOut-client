#!/usr/bin/env python3
"""Build the private Linux preview and add a separate application-menu launcher."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--api-origin', default='https://findout-backend.vercel.app')
args = parser.parse_args()
repo = Path(__file__).resolve().parent.parent
env = dict(os.environ, FINDOUT_API_ORIGIN=args.api_origin, FINDOUT_GITHUB_REPOSITORY='')
subprocess.run(['cargo', 'build', '--locked', '--release', '--features', 'local-trial'],
               cwd=repo, env=env, check=True)
trial = repo / 'target' / 'monolith'
trial.mkdir(parents=True, exist_ok=True)
binary = trial / 'findout-monolith'
# Replace atomically so rebuilding also works while the previous preview is running.
staged = trial / 'findout-monolith.new'
shutil.copy2(repo / 'target' / 'release' / 'findout-client', staged)
staged.replace(binary)
data = Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local' / 'share')))
if not data.is_absolute():
    raise SystemExit('XDG_DATA_HOME must be an absolute path')
applications = data / 'applications'
applications.mkdir(parents=True, exist_ok=True)
# Desktop Entry Exec quoting, including its second layer of backslash parsing.
def exec_quote(path):
    value = str(path).replace('\\', '\\\\\\\\').replace('"', '\\\\"').replace('`', '\\\\`').replace('$', '\\\\$').replace('%', '%%')
    return '"' + value + '"'
launcher = applications / 'findout-monolith.desktop'
launcher.write_text(
    '[Desktop Entry]\nType=Application\nName=FindOut Monolith\n'
    'Comment=Local preview of the FindOut desktop redesign\n'
    f'Exec={exec_quote(binary)}\n'
    f'Icon={repo / "assets" / "findout-tray.svg"}\n'
    'Terminal=false\nCategories=Utility;\nStartupNotify=false\n', encoding='utf-8')
print(f'Built: {binary}')
print(f'Launcher: {launcher}')
print('Quit the other FindOut instance from its tray menu, then open FindOut Monolith.')
