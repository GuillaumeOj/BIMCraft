# Se déplacer dans la vue 3D

La vue 3D, au centre de la fenêtre, montre votre modèle en volumes ombrés. Vous tournez autour, le faites glisser et zoomez avec la souris, un pavé tactile ou le clavier, et vous passez d'un clic à des vues types comme Dessus ou une vue isométrique.

## Avant de commencer

- Rien : la vue 3D s'ouvre avec le projet. Un projet vide affiche un fond gris et le message « Rien à afficher pour l'instant » ; [placez un bloc](place-a-block.fr.md) pour avoir quelque chose à regarder.

Quelques mots employés sur cette page :

- **Orbite.** Tourner la vue autour du modèle, comme si vous en faisiez le tour.
- **Panoramique.** Faire glisser la vue latéralement ou verticalement sans la tourner.
- **Projection perspective / parallèle.** En perspective, les objets lointains paraissent plus petits, comme sur une photo. En projection parallèle, ils gardent leur taille, comme sur un dessin technique.

## Se déplacer à la souris ou au pavé tactile

| Pour | Souris | Pavé tactile |
| --- | --- | --- |
| Orbite | Clic droit et glisser | Utilisez les flèches |
| Panoramique | Clic molette et glisser, ou Maj + glisser | Deux doigts |
| Zoom | Molette, ou Ctrl + molette | Pincer |

À la première ouverture, une carte **Se déplacer dans le modèle** rappelle ces gestes. **J'ai compris** la masque jusqu'au prochain démarrage de Fabrilo ; **Ne plus afficher** la masque définitivement.

## Se déplacer au clavier

1. Cliquez dans la vue 3D, ou appuyez sur **Tab** jusqu'à ce qu'elle ait un contour vert. Les lecteurs d'écran l'annoncent comme « Vue 3D ».
2. Utilisez les touches ci-dessous.
3. Appuyez sur **Tab** pour quitter la vue : elle ne garde jamais le clavier.

| Touche | Effet |
| --- | --- |
| Flèches | Orbite |
| Maj + flèches | Panoramique |
| `+` / `-` | Zoom avant / arrière |
| `F` | Tout cadrer : montre tout le modèle |
| Maj + `F` | Cadrer la sélection : montre les éléments sélectionnés |
| `P` | Passer de la perspective à la projection parallèle et inversement |

**Résultat :** la vue bouge aussitôt ; rien ne change dans le modèle.

## Utiliser les commandes de vue

Les commandes de vue sont en bas à droite de la fenêtre, de gauche à droite :

| Commande | Effet |
| --- | --- |
| **Tout cadrer** | Montre tout le modèle. **Raccourci :** `F`. Indisponible tant que le modèle est vide. |
| **Cadrer la sélection** | Montre les éléments sélectionnés. **Raccourci :** Maj + `F`. Indisponible sans sélection. |
| **Vues prédéfinies** | Ouvre un menu de vues types : Dessus, Avant, Arrière, Gauche, Droite et quatre vues isométriques (Iso sud-est, sud-ouest, nord-est, nord-ouest). |
| **Perspective / Parallèle** | Choisit la projection. **Raccourci :** `P`. |

L'infobulle de chaque commande indique son raccourci. La barre d'état, en bas à gauche, indique ce qui est sélectionné et si l'accroche est activée.

## Désactiver les raccourcis à une touche

Les raccourcis d'une seule lettre (`V`, `M`, `B`, `P`, `S`) peuvent être déclenchés par erreur, par exemple par un logiciel de reconnaissance vocale. Pour les désactiver, décochez **Raccourcis à une touche** dans la barre de titre, à côté du choix de la langue. Fabrilo retient ce choix.

**Résultat :** ces lettres ne fonctionnent plus que lorsque la vue 3D a le clavier (cliquez dedans, ou atteignez-la avec Tab). Les raccourcis avec une touche de modification, comme Ctrl + Maj + M, et les touches propres à la vue 3D (flèches, `F`, Échap, valeurs saisies) restent actifs. Les infobulles ne citent plus les raccourcis à une touche.

## Astuces

- Après une vue prédéfinie, appuyez sur `F` pour y cadrer tout le modèle.
- Les lecteurs d'écran annoncent la vue choisie et la projection, par exemple « Vue Dessus » ou « Projection Parallèle ».

## Dépannage

**La vue 3D affiche « La vue 3D n'est pas disponible sur cet ordinateur ».**
Fabrilo affiche la 3D avec votre carte graphique grâce à une fonction du navigateur appelée WebGPU. Certains ordinateurs ne la proposent pas encore, notamment sous Linux. Votre projet, ses panneaux et l'édition restent disponibles ; seule la vue 3D manque. Mettez à jour votre système et vos pilotes graphiques ; sous Linux, une prochaine version de Fabrilo affichera la 3D sans WebGPU.

**La vue 3D affiche « La vue 3D s'est arrêtée ».**
La carte graphique ne répond plus, ou le modèle est trop volumineux pour elle. Votre modèle est intact. Choisissez **Recharger la vue**. Si cela recommence peu après, redémarrez Fabrilo. **Copier les détails** copie une courte description technique (sans contenu du modèle) pour signaler le problème.

**La vue reste sur « Chargement de la vue 3D… ».**
Au bout de dix secondes, elle affiche « Toujours en cours… » et propose **Annuler**. Annulez, puis choisissez **Recharger la vue**.

**Un lien vers le guide ne s'ouvre pas.**
Fabrilo ouvre les pages du guide dans votre navigateur web et a besoin d'une connexion internet pour cela. S'il n'y parvient pas, il affiche l'adresse de la page pour que vous puissiez la copier dans votre navigateur.

**Les lettres comme `P` ou `S` ne font rien.**
Ces raccourcis sont inactifs quand une boîte de dialogue est ouverte ou pendant la saisie dans un champ. Fermez la boîte de dialogue ou cliquez d'abord dans la vue 3D. Si **Raccourcis à une touche** est décoché, elles ne fonctionnent que lorsque la vue 3D a le clavier (voir plus haut).

## Voir aussi

- [Sélectionner et déplacer des éléments](select-and-move-elements.fr.md)
- [Placer un bloc](place-a-block.fr.md)
