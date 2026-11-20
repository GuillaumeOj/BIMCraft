# Update Fabrilo

Fabrilo can look for a newer version, download it and install it for you. It never does anything without your say-so: it only checks for a newer version, and it asks before it restarts.

## Before you start

- Fabrilo must have been installed with its installer (see [Install Fabrilo](getting-started.md)). A copy of Fabrilo that is run by developers from source code has no update controls: the **Updates** button is simply not there.
- On Windows, only a copy installed with the `.exe` installer updates itself. A copy installed with the MSI package has no **Updates** button either (see below).
- You need an internet connection to check and download. Without one, Fabrilo keeps working and you can update later.

## Steps

### Let Fabrilo check for you

By default, Fabrilo checks once each time it starts, a few seconds after the window opens. If a newer version exists, the **Updates** button in the title bar turns into a green **Update available** button. Nothing else happens: no window opens and nothing is downloaded. If the check fails (for example because you are offline), Fabrilo says nothing.

### Check now

1. Choose **Updates** in the title bar (or press **Ctrl+Shift+U**, Cmd+Shift+U on macOS). The **Updates** window opens.
2. Choose **Check now**.

**Result:** the window says either "Fabrilo *version* is up to date" or "Fabrilo *version* is available", with what is new in that version. If the check fails, the window says why and offers **Try again**.

### Install an update

1. Open the **Updates** window and read **What's new**.
2. Choose **Download and install**. A progress bar shows the download. You can close the window: the download continues, and the title bar button shows the percentage.
3. When the download is finished, the button reads **Restart to update**. Open **Updates** and choose **Restart now**. Choose **Later** if you are in the middle of something: the update waits for you.
4. Fabrilo asks you to confirm. Choose **Restart now** to continue, or **Cancel** to stay in the current version.

**Result:** Fabrilo closes and reopens in the new version. Before it installs anything, Fabrilo checks that the download really comes from the Fabrilo team; a package that cannot be verified is never installed.

**Before you restart.** Restarting closes Fabrilo. Open windows and changes you have not applied yet, such as a build-up you are still editing, are lost. Fabrilo keeps a recovery copy of your work while you edit, so you can restore it after the restart; see [Recover unsaved work after a crash](recover-unsaved-work.md) for when this is available.

### Turn the automatic check off

1. Open the **Updates** window.
2. Switch off **Check for updates automatically**.

**Result:** Fabrilo no longer looks for updates by itself. **Check now** still works. Fabrilo remembers this choice on your computer.

### Choose Stable or Beta

The **Update channel** decides which versions Fabrilo offers.

| Channel | What it offers |
| --- | --- |
| Stable | Releases that have been tested. Recommended. |
| Beta | Early access to new features. It may contain bugs. |

Choosing a channel starts a check on that channel at once. You cannot change the channel while Fabrilo is checking, downloading or waiting for a restart. Fabrilo remembers your choice.

**Going back from Beta to Stable.** Fabrilo never installs an older version, so choosing **Stable** does not remove the beta you are running. The beta stays installed until a Stable version that is newer than it is released; Fabrilo then offers that version. To go back sooner, install the Stable version from the Releases page of the Fabrilo project on GitHub.

**If you installed with the MSI package.** The MSI package (the all-users installation for IT departments) does not update itself: Fabrilo shows no **Updates** button. Your IT department installs each new version with its new MSI package. To get updates automatically, install Fabrilo with the `.exe` installer instead (see [Install Fabrilo](getting-started.md)). Beta versions come as the `.exe` installer only.

## What the check sends

The check asks for one public file on the project's website, over a secure connection, to learn the latest version. It sends no project data, no account information and no version number or identifier; the server sees your internet address, like any website you visit. Fabrilo never downloads or installs anything without you choosing **Download and install**.

## Options

| Option | What it does |
| --- | --- |
| Update channel | **Stable** or **Beta**: which versions are offered. |
| Check for updates automatically | Looks for an update once at each start. On by default. |
| Check now | Looks for an update at once and shows the result or the error. |
| Download and install | Downloads the new version and verifies it. It does not restart Fabrilo. |
| Restart now | Closes Fabrilo and installs the downloaded update, after a confirmation. |
| Later | Closes the window. The update stays available. |

## Troubleshooting

**"Could not reach the update server. Check your internet connection and try again."**
Fabrilo is offline, or a firewall or proxy blocks it. Check your connection, then choose **Try again**. On a company network, ask your IT department to allow access to the Fabrilo project's website and to GitHub.

**"The downloaded update could not be verified, so it was not installed."**
The download was damaged or altered on its way. Your current version is unchanged and safe to keep using. Choose **Try again** later. If it keeps happening, download the installer again from the Releases page of the Fabrilo project on GitHub.

**"Fabrilo could not install the update."**
Choose **Try again**. If it still fails, download the installer from the Releases page and run it: it replaces the old version and keeps your settings. Your current version is unchanged until the installation succeeds.

**"The update information could not be read right now."**
Try again later. The update information is published with each new version; if the message stays for days, download the latest installer from the Releases page of the Fabrilo project on GitHub.

**There is no Updates button.**
This copy of Fabrilo cannot update itself: a development copy, or a copy installed with the MSI package (see above). Install Fabrilo with the `.exe` installer on Windows or the `.dmg` on macOS.

## See also

- [Install Fabrilo](getting-started.md)
- [Use the workspace with the keyboard](use-the-workspace-with-the-keyboard.md)
- [Recover unsaved work after a crash](recover-unsaved-work.md)
