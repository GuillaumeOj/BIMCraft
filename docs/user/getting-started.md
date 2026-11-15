# Install Fabrilo

Download Fabrilo from the project's Releases page and install it on Windows or macOS. The installers are signed, so Windows and macOS can check that they come from the Fabrilo project and have not been changed.

## Before you start

Check that your computer can run Fabrilo:

| | Windows | macOS |
| --- | --- | --- |
| System | Windows 10 (version 1809 or later) or Windows 11, 64-bit | macOS 11 or later |
| Processor | x64 (Intel or AMD). Windows on ARM runs it through emulation. | Apple silicon or Intel |
| Internet | To download, and once during installation if WebView2 is missing (see Troubleshooting) | To download |

On Windows, Fabrilo displays its interface with **Microsoft Edge WebView2**, a component that Windows 11 and most Windows 10 computers already have. The installer adds it when it is missing.

## Steps

### Download the installer

1. Open the Releases page of the Fabrilo project: <https://github.com/GuillaumeOj/BIMCraft/releases>.
2. Under the newest release, open **Assets** and download the file for your computer:

| Your computer | File to download |
| --- | --- |
| Windows, for yourself | `Fabrilo_<version>_x64-setup.exe` (recommended) |
| Windows, for all users, deployed by an IT department | `Fabrilo_<version>_x64_en-US.msi` |
| Mac (Apple silicon or Intel) | `Fabrilo_<version>_universal.dmg` |

**Result:** the file is in your Downloads folder.

### Install on Windows with the `.exe` installer

1. Double-click `Fabrilo_<version>_x64-setup.exe`.
2. Follow the installer: keep the proposed folder or choose another, then choose **Install**. No administrator rights are needed: Fabrilo is installed for your Windows account only.
3. Choose **Finish**.

**Result:** Fabrilo is in the Start menu. This copy keeps itself up to date (see [Update Fabrilo](update-fabrilo.md)).

### Install on Windows with the MSI package

The MSI package installs Fabrilo for every user of the computer. It is meant for IT departments, which can also deploy it silently with their usual tools.

1. Double-click `Fabrilo_<version>_x64_en-US.msi`, or deploy it with your management tool.
2. Follow the installer. Windows asks for administrator rights.

**Result:** Fabrilo is in the Start menu of every user. A copy installed with the MSI package **does not update itself** and has no **Updates** button: install each new version with its new MSI package. Install with the `.exe` installer instead if you want automatic updates.

### Install on macOS

1. Double-click `Fabrilo_<version>_universal.dmg`. A window opens with the Fabrilo icon and a shortcut to your **Applications** folder.
2. Drag the Fabrilo icon onto **Applications**.
3. Eject the disk image: in Finder, choose the eject button next to **Fabrilo** in the sidebar.

**Result:** Fabrilo is in your Applications folder and in Launchpad.

### Start Fabrilo for the first time

1. Windows: open the Start menu and choose **Fabrilo**. macOS: open **Applications** and double-click **Fabrilo**.
2. On macOS, the first time, a message asks whether you want to open an application downloaded from the internet. Choose **Open**.

**Result:** the Fabrilo window opens. A few seconds later, Fabrilo checks once whether a newer version exists; it never downloads anything without asking you.

## Options

### Choose beta versions

Beta versions give early access to new features and may contain bugs. They are marked **Pre-release** on the Releases page, and their version ends with `-beta` and a number (for example `0.4.0-beta.1`).

- To try one, download it from the Releases page as above. On Windows, beta versions come as the `.exe` installer only.
- To be offered beta versions as updates, choose the **Beta** update channel in the **Updates** window (see [Update Fabrilo](update-fabrilo.md)).

### Check the download (optional)

Each release has a file `SHA256SUMS.txt` with a fingerprint (a SHA-256 checksum) of every file. Comparing it with the fingerprint of your download proves the file arrived intact.

1. Download `SHA256SUMS.txt` from the same release.
2. Compute the fingerprint of your download:
   - Windows: in PowerShell, `Get-FileHash .\Downloads\Fabrilo_<version>_x64-setup.exe`
   - macOS: in Terminal, `shasum -a 256 ~/Downloads/Fabrilo_<version>_universal.dmg`
3. Compare it with the line of that file in `SHA256SUMS.txt` (upper or lower case does not matter).

**Result:** the two fingerprints are identical. If they differ, delete the file and download it again.

### Uninstall Fabrilo

- Windows: Settings → **Apps** → **Installed apps** → **Fabrilo** → **Uninstall**.
- macOS: drag **Fabrilo** from **Applications** to the Trash.

Your projects are files you saved yourself; uninstalling Fabrilo does not delete them.

## Troubleshooting

**Windows shows "Windows protected your PC" (SmartScreen).**
Windows shows this message for programs it does not know well yet, even signed ones, typically in the first days after a new version. Choose **More info**: the publisher must read **SignPath Foundation**, the organisation that signs Fabrilo for the project. If it does, choose **Run anyway**. If the publisher is unknown or different, do not run the file: delete it and download it again from the Releases page.

**Windows shows "Unknown publisher" when you uninstall Fabrilo.**
The uninstaller is created on your computer during installation, so it cannot carry the project's signature. You can continue safely.

**macOS says that Fabrilo "cannot be opened" or "is damaged".**
The download was incomplete or altered. Move the file to the Trash, download the `.dmg` again from the Releases page and check it (see "Check the download"). Do not bypass the message.

**The Windows installer says it cannot install WebView2.**
The installer downloads WebView2 when it is missing, so it needs an internet connection at that moment. Connect to the internet and run the installer again. On a computer without internet access, ask your IT department to install the **Microsoft Edge WebView2 Runtime** first.

**There is no Updates button.**
A copy installed with the MSI package does not update itself. Install the new version with its MSI package, or use the `.exe` installer to get automatic updates.

## See also

- [Update Fabrilo](update-fabrilo.md)
- [Change the interface language](change-the-interface-language.md)
- [Save and open a project](save-and-open-a-project.md)
