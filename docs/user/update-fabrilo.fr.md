# Mettre à jour Fabrilo

Fabrilo peut chercher une version plus récente, la télécharger et l’installer pour vous. Il ne fait jamais rien sans votre accord : il se contente de chercher une version plus récente, et il demande avant de redémarrer.

## Avant de commencer

- Fabrilo doit avoir été installé avec son installateur (voir [Installer Fabrilo](getting-started.md)). Une copie de Fabrilo lancée par des développeurs à partir du code source n’a pas de commandes de mise à jour : le bouton **Mises à jour** n’apparaît tout simplement pas.
- Il faut une connexion internet pour chercher et télécharger. Sans elle, Fabrilo continue de fonctionner et vous pourrez mettre à jour plus tard.

## Étapes

### Laisser Fabrilo chercher à votre place

Par défaut, Fabrilo cherche une mise à jour une fois à chaque démarrage, quelques secondes après l’ouverture de la fenêtre. S’il existe une version plus récente, le bouton **Mises à jour** de la barre de titre devient un bouton vert **Mise à jour disponible**. Rien d’autre ne se passe : aucune fenêtre ne s’ouvre et rien n’est téléchargé. Si la recherche échoue (par exemple parce que vous êtes hors ligne), Fabrilo ne dit rien.

### Rechercher maintenant

1. Choisissez **Mises à jour** dans la barre de titre (ou appuyez sur **Ctrl+Maj+U**, Cmd+Maj+U sous macOS). La fenêtre **Mises à jour** s’ouvre.
2. Choisissez **Rechercher maintenant**.

**Résultat :** la fenêtre indique soit « Fabrilo *version* est à jour », soit « Fabrilo *version* est disponible », avec les nouveautés de cette version. Si la recherche échoue, la fenêtre en donne la raison et propose **Réessayer**.

### Installer une mise à jour

1. Ouvrez la fenêtre **Mises à jour** et lisez les **Nouveautés**.
2. Choisissez **Télécharger et installer**. Une barre de progression montre le téléchargement. Vous pouvez fermer la fenêtre : le téléchargement continue, et le bouton de la barre de titre affiche le pourcentage.
3. Quand le téléchargement est terminé, le bouton indique **Redémarrer pour mettre à jour**. Ouvrez **Mises à jour** et choisissez **Redémarrer maintenant**. Choisissez **Plus tard** si vous êtes au milieu d’une tâche : la mise à jour attend.
4. Fabrilo demande une confirmation. Choisissez **Redémarrer maintenant** pour continuer, ou **Annuler** pour rester dans la version actuelle.

**Résultat :** Fabrilo se ferme puis se rouvre dans la nouvelle version. Avant d’installer quoi que ce soit, Fabrilo vérifie que le téléchargement vient bien de l’équipe Fabrilo ; un paquet qui ne peut pas être vérifié n’est jamais installé.

**Avant de redémarrer.** Le redémarrage ferme Fabrilo. Les fenêtres ouvertes et les modifications pas encore appliquées, comme une composition en cours d’édition, sont perdues. Fabrilo garde une copie de récupération de votre travail pendant que vous le modifiez, ce qui permet de le restaurer après le redémarrage ; voir [Récupérer un travail non enregistré](recover-unsaved-work.md) pour savoir quand cette fonction est disponible.

### Désactiver la recherche automatique

1. Ouvrez la fenêtre **Mises à jour**.
2. Désactivez **Rechercher les mises à jour automatiquement**.

**Résultat :** Fabrilo ne cherche plus de mise à jour tout seul. **Rechercher maintenant** fonctionne toujours. Fabrilo retient ce choix sur votre ordinateur.

### Choisir Stable ou Bêta

Le **Canal de mise à jour** détermine les versions que Fabrilo propose.

| Canal | Ce qu’il propose |
| --- | --- |
| Stable | Des versions qui ont été testées. Recommandé. |
| Bêta | Un accès anticipé aux nouvelles fonctions. Elle peut contenir des bogues. |

Choisir un canal lance aussitôt une recherche sur ce canal. Le canal ne peut pas changer pendant la recherche, le téléchargement ou l’attente d’un redémarrage. Fabrilo retient votre choix.

**Revenir de Bêta à Stable.** Fabrilo n’installe jamais une version plus ancienne : choisir **Stable** ne retire donc pas la bêta que vous utilisez. La bêta reste installée jusqu’à la publication d’une version Stable plus récente qu’elle ; Fabrilo vous propose alors cette version. Pour revenir plus tôt, installez la version Stable depuis la page des versions (Releases) du projet Fabrilo sur GitHub.

**Si vous avez installé avec le paquet MSI.** Le paquet MSI (l’installation pour tous les utilisateurs, destinée aux services informatiques) ne reçoit que les mises à jour Stable. Sur Bêta, sa recherche se termine par « Les informations de mise à jour n’ont pas pu être lues pour le moment ». Choisissez **Stable**. Les versions bêta ne sont fournies que par l’installateur `.exe`.

## Ce que la recherche envoie

La recherche demande un seul fichier public du site du projet, par une connexion sécurisée, pour connaître la dernière version. Elle n’envoie aucune donnée de projet, aucune information de compte, ni numéro de version ni identifiant ; le serveur voit votre adresse internet, comme tout site web que vous visitez. Fabrilo ne télécharge et n’installe jamais rien sans que vous choisissiez **Télécharger et installer**.

## Options

| Option | Effet |
| --- | --- |
| Canal de mise à jour | **Stable** ou **Bêta** : les versions proposées. |
| Rechercher les mises à jour automatiquement | Cherche une mise à jour une fois à chaque démarrage. Activé par défaut. |
| Rechercher maintenant | Cherche une mise à jour aussitôt et montre le résultat ou l’erreur. |
| Télécharger et installer | Télécharge la nouvelle version et la vérifie. Ne redémarre pas Fabrilo. |
| Redémarrer maintenant | Ferme Fabrilo et installe la mise à jour téléchargée, après confirmation. |
| Plus tard | Ferme la fenêtre. La mise à jour reste disponible. |

## Dépannage

**« Impossible de joindre le serveur de mises à jour. Vérifiez votre connexion internet et réessayez. »**
Fabrilo est hors ligne, ou un pare-feu ou un proxy le bloque. Vérifiez votre connexion, puis choisissez **Réessayer**. Sur un réseau d’entreprise, demandez à votre service informatique d’autoriser l’accès au site du projet Fabrilo et à GitHub.

**« La mise à jour téléchargée n’a pas pu être vérifiée ; elle n’a donc pas été installée. »**
Le téléchargement a été endommagé ou modifié en route. Votre version actuelle est inchangée et reste utilisable. Choisissez **Réessayer** plus tard. Si cela se reproduit, téléchargez de nouveau l’installateur depuis la page des versions (Releases) du projet Fabrilo sur GitHub.

**« Fabrilo n’a pas pu installer la mise à jour. »**
Choisissez **Réessayer**. Si cela échoue encore, téléchargez l’installateur depuis la page des versions et lancez-le : il remplace l’ancienne version et garde vos réglages. Votre version actuelle reste inchangée tant que l’installation n’a pas réussi.

**« Les informations de mise à jour n’ont pas pu être lues pour le moment. »**
Réessayez plus tard. Si vous utilisez Bêta avec une installation MSI, passez sur **Stable** (voir plus haut).

**Il n’y a pas de bouton Mises à jour.**
Cette copie de Fabrilo ne peut pas se mettre à jour elle-même (par exemple une copie de développement). Installez Fabrilo avec son installateur.

## Voir aussi

- [Installer Fabrilo](getting-started.md)
- [Utiliser l’espace de travail au clavier](use-the-workspace-with-the-keyboard.md)
- [Récupérer un travail non enregistré](recover-unsaved-work.md)
