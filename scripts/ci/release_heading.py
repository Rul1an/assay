"""Dated changelog headings, not an authorization to install a release."""

import re


RELEASE_HEADING = re.compile(
    r"## \[(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-(?:rc|beta)\.[0-9]+)?\] - [0-9]{4}-[0-9]{2}-[0-9]{2}"
)
