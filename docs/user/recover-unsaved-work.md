# Recover unsaved work after a crash

BIMCraft saves a recovery copy of your work at regular intervals while you edit. If the program or the computer stops before you save, for example after a power cut, you can bring back your changes from that copy the next time you open the project.

## Before you start

- BIMCraft does not have a release yet. The menu entries, shortcuts and command-palette names will be added to this page with the first version that can autosave from the interface.

## How autosave works

- **When it saves.** Autosave starts counting when you make your first change after a save. When the autosave interval has passed, it writes a recovery copy. The countdown does not start again with every change, so you never lose more than one interval of work.
- **Where it saves.** The recovery copy is a separate file kept next to your project. Autosave never changes your project file: the project file only changes when you save it yourself.
- **When the copy is removed.** Saving the project yourself deletes the recovery copy, because the project then holds all your work.

## Change the autosave interval

The interval is in whole minutes, from **1 to 60**. It is **5 minutes** by default. A shorter interval loses less work after a crash; a longer one interrupts very large models less often. Autosave cannot be turned off.

## Recover your work after a crash

1. Start BIMCraft again and open the project you were working on.
2. If BIMCraft finds a recovery copy, it tells you when the copy was saved and asks whether to restore it.
3. Choose to restore it to continue from the recovery copy, or to discard it to keep the project as you last saved it.
4. If you restored the copy, save the project to keep the recovered work in the project file.

**Result:** the model shows your work as it was at the last autosave. Changes made after that autosave are lost.

## Tips

- Save often anyway. Autosave protects against crashes; it does not replace saving or keeping backups.
- If you want to undo everything since your last save, close the project without saving and discard the recovery copy when BIMCraft offers it.

## Troubleshooting

**BIMCraft offers to restore a recovery copy every time I open the project**
The copy is only removed when you save the project. Restore or discard it, then save the project.

**The recovery copy cannot be opened ("The project file is damaged")**
The copy was damaged, for example by a disk that was full or removed during the crash. Discard it and open the project as you last saved it. Autosave writes each copy in one step, so a crash *during* an autosave keeps the previous copy instead.

**Autosave reports that it could not save**
The disk is probably full or no longer available. Free some space or reconnect the disk. BIMCraft tries again after one interval; your changes stay in the program until then.

## See also

- [Save and open a project](save-and-open-a-project.md)
