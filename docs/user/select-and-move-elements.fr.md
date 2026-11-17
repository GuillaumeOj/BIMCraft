# Sélectionner et déplacer des éléments

Sélectionnez des éléments dans la vue 3D pour les mettre en évidence, puis déplacez-les, en les faisant glisser ou précisément avec l'outil Déplacer et une distance saisie. Les déplacements s'accrochent aux angles, aux milieux d'arêtes, aux arêtes, aux faces et au sol, et chaque déplacement peut être annulé.

## Avant de commencer

- Le modèle contient au moins un élément, par exemple un [bloc](place-a-block.fr.md).

Quelques mots employés sur cette page :

- **Accroche.** Pendant que vous pointez, BIMCraft capte le point remarquable le plus proche à quelques pixels près : une extrémité (un angle), le milieu d'une arête, un point d'une arête ou d'une face, ou le sol. Un repère et une étiquette indiquent lequel.
- **Point de départ / point d'arrivée.** Le point d'où part le déplacement et celui où il arrive.

## Sélectionner des éléments

1. Avec l'outil **Sélection** (**Raccourci :** `V`), passez le pointeur sur le modèle : l'élément survolé prend un contour orange.
2. Cliquez sur un élément pour le sélectionner. Il prend un contour et une teinte vert-bleu.
3. Pour ajouter des éléments, cliquez dessus avec Maj. Pour ajouter ou retirer un élément, cliquez dessus avec Ctrl (Cmd sur macOS).
4. Pour vider la sélection, cliquez sur une partie vide de la vue ou appuyez sur **Échap** dans la vue.

**Résultat :** la barre d'état, en bas à gauche, affiche par exemple « 2 éléments sélectionnés · Échap pour effacer », et les lecteurs d'écran annoncent le nombre.

Il n'est pas encore possible de désigner un élément au clavier seul ; les utilisateurs du clavier sélectionneront depuis l'arborescence du projet quand elle existera.

## Déplacer en faisant glisser

1. Faites glisser un élément sélectionné (un élément non sélectionné est d'abord sélectionné). Un fantôme reste à la position de départ pendant que l'élément suit le pointeur et s'accroche aux points des autres éléments.
2. Relâchez le bouton pour le déposer.

**Résultat :** l'élément est déplacé et un message comme « 1 élément déplacé de 4,000 m » apparaît avec un bouton **Annuler**.

## Déplacer précisément avec l'outil Déplacer

1. Sélectionnez les éléments à déplacer.
2. Choisissez **Déplacer** dans la barre d'outils en bas. **Raccourci :** `M`. Sans sélection, BIMCraft affiche « Sélectionnez d'abord un élément à déplacer ».
3. Cliquez sur le point de départ ; l'accroche vous aide à prendre un angle ou un milieu.
4. Pointez dans la direction du déplacement. Une ligne élastique et une étiquette montrent la distance.
5. Cliquez sur le point d'arrivée, ou saisissez la distance en mètres (par exemple `2.5`, avec un point comme séparateur décimal) et appuyez sur **Entrée**.

**Résultat :** les éléments sont déplacés et un message comme « 1 élément déplacé de 2,500 m » apparaît avec un bouton **Annuler**.

Appuyez sur **Échap** pour abandonner un déplacement en cours. Si vous saisissez une distance, le premier **Échap** l'efface seulement : le déplacement garde son point de base et suit de nouveau le pointeur ; un second **Échap** l'abandonne.

## Options

| Option | Effet |
| --- | --- |
| Maintenir **Alt** | Suspend l'accroche tant que la touche est maintenue. |
| `S` | Active ou désactive l'accroche. La barre d'état affiche « Accroche activée » ou « Accroche désactivée ». |
| **Annuler** dans le message | Remet les éléments à leur place. |

## Dépannage

**« Cet élément ne peut pas encore être déplacé ».**
Ce type d'élément ne peut pas encore être déplacé par cet outil. Seuls les blocs se déplacent pour l'instant.

**« Trop d'éléments à déplacer en une fois ».**
Un déplacement prend au plus 10 000 éléments. Sélectionnez-en moins et déplacez-les en plusieurs fois.

**Le déplacement s'accroche au mauvais point.**
Zoomez pour écarter les points à l'écran, ou maintenez **Alt** pour placer librement.

## Voir aussi

- [Se déplacer dans la vue 3D](navigate-the-3d-view.fr.md)
- [Placer un bloc](place-a-block.fr.md)
