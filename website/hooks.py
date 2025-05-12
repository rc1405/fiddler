import os
import posixpath
from pathlib import Path
import mkdocs.plugins


@mkdocs.plugins.event_priority(-50)
def on_config(config, **kwargs):
    docs_dir = config["docs_dir"]
    nav_items = []

    # Prioritize Nav items
    for i in ["inputs", "processors", "outputs"]:
        items = create_nav(docs_dir, os.path.join(docs_dir, i))
        nav_items.append({format_title(i): items})

    # ignore processed files
    excluded = [
        os.path.join(docs_dir, "inputs"),
        os.path.join(docs_dir, "processors"),
        os.path.join(docs_dir, "outputs"),
        os.path.join(docs_dir, "index.md"),
        os.path.join(docs_dir, "getting_started.md"),
        os.path.join(docs_dir, "configuration.md"),
        os.path.join(docs_dir, "development.md"),
    ]

    nav_items.extend(create_nav(docs_dir, docs_dir, excluded=excluded))

    if isinstance(config["nav"], list):
        config["nav"].extend(nav_items)
    else:
        config["nav"] = nav_items

    return config


@mkdocs.plugins.event_priority(-50)
def rename_item(path):
    file = os.path.basename(path)
    file = remove_prefix(file)

    parent_path = os.path.dirname(path)
    if parent_path != path:
        parent_path = rename_item(parent_path)
    else:
        if os.path.isabs(path):
            parent_path = os.path.dirname(path)
        else:
            parent_path = ""
    return os.path.join(parent_path, file)


@mkdocs.plugins.event_priority(-50)
def on_files(files, config):
    for file in files:
        file.dest_path = rename_item(file.dest_path)
        file.dest_uri = Path(rename_item(file.dest_path)).as_posix()
        file.url = Path(rename_item(file.url)).as_posix()
        file.abs_dest_path = rename_item(file.abs_dest_path)

    return files


def create_nav(base_dir, path, excluded=[]):
    nav_dict = []
    for i in sorted(os.listdir(os.path.join(base_dir, path))):
        item_path = posixpath.join(path, i)

        if (
            os.path.join(base_dir, path) in excluded
            or os.path.join(base_dir, path, i) in excluded
        ):
            continue

        if os.path.isfile(item_path) and not i.endswith(".md"):
            continue

        item_title = os.path.splitext(i)[0]
        if item_title.startswith("_"):
            continue

        item_title = remove_prefix(item_title)
        if len(excluded) > 0:
            item_title = format_title(item_title)

        if os.path.isfile(item_path):
            nav_dict.append({item_title: posixpath.relpath(item_path, base_dir)})

        elif os.path.isdir(item_path):
            item_dict = create_nav(base_dir, item_path, excluded=excluded)
            if item_dict:
                nav_dict.append({format_title(item_title): item_dict})
    return nav_dict


def remove_prefix(item_title):
    if len(item_title) >= 3 and item_title[0:2].isdigit() and item_title[2] == "_":
        item_title = item_title[3:]
    return item_title


def format_title(title):
    formatted = " ".join(
        [
            word.capitalize()
            for word in title.replace("-", " ").replace("_", " ").split()
        ]
    )
    return f"{formatted}"
