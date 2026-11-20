# Installer Fabrilo

Téléchargez Fabrilo depuis la page des versions (Releases) du projet et installez-le sous Windows ou macOS. Les installateurs sont signés : Windows et macOS peuvent ainsi vérifier qu’ils viennent du projet Fabrilo et n’ont pas été modifiés.

## Avant de commencer

Vérifiez que votre ordinateur peut faire fonctionner Fabrilo :

| | Windows | macOS |
| --- | --- | --- |
| Système | Windows 10 (version 1809 ou ultérieure) ou Windows 11, 64 bits | macOS 11 ou ultérieur |
| Processeur | x64 (Intel ou AMD). Windows sur ARM le fait fonctionner par émulation. | Apple silicon ou Intel |
| Internet | Pour le téléchargement, et une fois pendant l’installation si WebView2 manque (voir Dépannage) | Pour le téléchargement |

Sous Windows, Fabrilo affiche son interface avec **Microsoft Edge WebView2**, un composant que Windows 11 et la plupart des ordinateurs sous Windows 10 possèdent déjà. L’installateur l’ajoute s’il manque.

## Étapes

### Télécharger l’installateur

1. Ouvrez la page des versions du projet Fabrilo : <https://github.com/GuillaumeOj/BIMCraft/releases>.
2. Sous la version la plus récente, ouvrez **Assets** et téléchargez le fichier qui correspond à votre ordinateur :

| Votre ordinateur | Fichier à télécharger |
| --- | --- |
| Windows, pour vous | `Fabrilo_<version>_x64-setup.exe` (recommandé) |
| Windows, pour tous les utilisateurs, déployé par un service informatique | `Fabrilo_<version>_x64_en-US.msi` |
| Mac (Apple silicon ou Intel) | `Fabrilo_<version>_universal.dmg` |

**Résultat :** le fichier se trouve dans votre dossier Téléchargements.

### Installer sous Windows avec l’installateur `.exe`

1. Double-cliquez sur `Fabrilo_<version>_x64-setup.exe`.
2. Suivez l’installateur : gardez le dossier proposé ou choisissez-en un autre, puis choisissez **Installer**. Aucun droit d’administrateur n’est nécessaire : Fabrilo est installé pour votre seul compte Windows.
3. Choisissez **Terminer**.

**Résultat :** Fabrilo figure dans le menu Démarrer. Cette copie se tient à jour elle-même (voir [Mettre à jour Fabrilo](update-fabrilo.fr.md)).

### Installer sous Windows avec le paquet MSI

Le paquet MSI installe Fabrilo pour tous les utilisateurs de l’ordinateur. Il est destiné aux services informatiques, qui peuvent aussi le déployer sans intervention avec leurs outils habituels.

1. Double-cliquez sur `Fabrilo_<version>_x64_en-US.msi`, ou déployez-le avec votre outil de gestion.
2. Suivez l’installateur. Windows demande des droits d’administrateur.

**Résultat :** Fabrilo figure dans le menu Démarrer de chaque utilisateur. Une copie installée avec le paquet MSI **ne se met pas à jour elle-même** et n’a pas de bouton **Mises à jour** : installez chaque nouvelle version avec son nouveau paquet MSI. Installez plutôt avec l’installateur `.exe` si vous voulez les mises à jour automatiques.

### Installer sous macOS

1. Double-cliquez sur `Fabrilo_<version>_universal.dmg`. Une fenêtre s’ouvre avec l’icône de Fabrilo et un raccourci vers votre dossier **Applications**.
2. Faites glisser l’icône de Fabrilo sur **Applications**.
3. Éjectez l’image disque : dans le Finder, choisissez le bouton d’éjection à côté de **Fabrilo** dans la barre latérale.

**Résultat :** Fabrilo se trouve dans votre dossier Applications et dans le Launchpad.

### Démarrer Fabrilo pour la première fois

1. Sous Windows : ouvrez le menu Démarrer et choisissez **Fabrilo**. Sous macOS : ouvrez **Applications** et double-cliquez sur **Fabrilo**.
2. Sous macOS, la première fois, un message vous demande si vous voulez ouvrir une application téléchargée sur internet. Choisissez **Ouvrir**.

**Résultat :** la fenêtre de Fabrilo s’ouvre. Quelques secondes plus tard, Fabrilo cherche une fois s’il existe une version plus récente ; il ne télécharge jamais rien sans vous le demander.

## Options

### Choisir les versions bêta

Les versions bêta donnent un accès anticipé aux nouvelles fonctions et peuvent contenir des bogues. Elles sont marquées **Pre-release** sur la page des versions, et leur numéro se termine par `-beta` suivi d’un nombre (par exemple `0.4.0-beta.1`).

- Pour en essayer une, téléchargez-la depuis la page des versions comme ci-dessus. Sous Windows, les versions bêta ne sont fournies que par l’installateur `.exe`.
- Pour que les versions bêta vous soient proposées en mise à jour, choisissez le canal de mise à jour **Bêta** dans la fenêtre **Mises à jour** (voir [Mettre à jour Fabrilo](update-fabrilo.fr.md)).

### Vérifier le téléchargement (facultatif)

Chaque version comprend un fichier `SHA256SUMS.txt` qui donne l’empreinte (une somme de contrôle SHA-256) de chaque fichier. La comparer avec l’empreinte de votre téléchargement prouve que le fichier est arrivé intact.

1. Téléchargez `SHA256SUMS.txt` depuis la même version.
2. Calculez l’empreinte de votre téléchargement :
   - Windows : dans PowerShell, `Get-FileHash .\Downloads\Fabrilo_<version>_x64-setup.exe`
   - macOS : dans Terminal, `shasum -a 256 ~/Downloads/Fabrilo_<version>_universal.dmg`
3. Comparez-la avec la ligne de ce fichier dans `SHA256SUMS.txt` (majuscules et minuscules sont équivalentes).

**Résultat :** les deux empreintes sont identiques. Si elles diffèrent, supprimez le fichier et téléchargez-le de nouveau.

### Désinstaller Fabrilo

- Windows : Paramètres → **Applications** → **Applications installées** → **Fabrilo** → **Désinstaller**.
- macOS : faites glisser **Fabrilo** du dossier **Applications** vers la Corbeille.

Vos projets sont des fichiers que vous avez enregistrés vous-même ; désinstaller Fabrilo ne les supprime pas.

## Dépannage

**Windows affiche « Windows a protégé votre ordinateur » (SmartScreen).**
Windows affiche ce message pour les programmes qu’il ne connaît pas encore bien, même signés, en général dans les premiers jours d’une nouvelle version. Choisissez **Informations complémentaires** : l’éditeur doit être **SignPath Foundation**, l’organisation qui signe Fabrilo pour le projet. Si c’est le cas, choisissez **Exécuter quand même**. Si l’éditeur est inconnu ou différent, n’exécutez pas le fichier : supprimez-le et téléchargez-le de nouveau depuis la page des versions.

**Windows affiche « Éditeur inconnu » quand vous désinstallez Fabrilo.**
Le programme de désinstallation est créé sur votre ordinateur pendant l’installation ; il ne peut donc pas porter la signature du projet. Vous pouvez continuer sans risque.

**macOS indique que Fabrilo « ne peut pas être ouvert » ou « est endommagé ».**
Le téléchargement est incomplet ou a été modifié. Placez le fichier dans la Corbeille, téléchargez de nouveau le `.dmg` depuis la page des versions et vérifiez-le (voir « Vérifier le téléchargement »). Ne contournez pas le message.

**L’installateur Windows indique qu’il ne peut pas installer WebView2.**
L’installateur télécharge WebView2 quand il manque : il lui faut donc une connexion internet à ce moment-là. Connectez-vous à internet et relancez l’installateur. Sur un ordinateur sans accès à internet, demandez à votre service informatique d’installer d’abord **Microsoft Edge WebView2 Runtime**.

**Il n’y a pas de bouton Mises à jour.**
Une copie installée avec le paquet MSI ne se met pas à jour elle-même. Installez la nouvelle version avec son paquet MSI, ou utilisez l’installateur `.exe` pour recevoir les mises à jour automatiquement.

## Voir aussi

- [Mettre à jour Fabrilo](update-fabrilo.fr.md)
- [Install Fabrilo (English)](getting-started.md)
