# Placer un bloc

Un bloc est une simple boîte : utilisez-le pour une étude de volumétrie, comme élément provisoire ou pour tout ce que vous modéliserez en détail plus tard. Vous le placez en deux clics pour son emprise et une hauteur, ou en saisissant ses dimensions.

## Avant de commencer

- Rien : on peut placer des blocs dans un projet vide.

Quelques mots employés sur cette page :

- **Emprise.** Le rectangle que le bloc occupe au sol : sa longueur et sa profondeur.
- **Accroche.** Pendant que vous pointez, BIMCraft capte l'angle, le milieu d'arête, l'arête, la face ou le sol le plus proche, et affiche un repère et son nom.

## Étapes

1. Choisissez **Bloc** dans la barre d'outils en bas de la fenêtre. **Raccourci :** `B`. Dans un projet vide, le bouton **Placer un bloc** au milieu de la vue fait de même.
2. Cliquez sur le premier angle de l'emprise, au sol ou sur un autre élément.
3. Cliquez sur le second angle, ou saisissez la longueur en mètres et appuyez sur **Entrée**, puis la profondeur et **Entrée**. Un rectangle en pointillés et sa taille (par exemple « 6,00 × 4,00 m ») suivent le pointeur.
4. Montez le pointeur pour régler la hauteur, ou saisissez-la et appuyez sur **Entrée**. **Entrée** sans valeur donne 3,00 m.

**Résultat :** le bloc apparaît, sélectionné, et un message comme « Bloc placé · 6,00 × 4,00 × 3,00 m » apparaît avec un bouton **Annuler**. L'outil Bloc reste actif pour placer le suivant ; appuyez sur **Échap** ou `V` pour revenir à la Sélection.

## Options

| Option | Effet |
| --- | --- |
| Valeurs saisies | Dimensions en mètres, avec un point comme séparateur décimal. Chaque valeur se termine par **Entrée**, jamais par Tab. |
| **Échap** | Revient d'une étape : de la hauteur à l'emprise, puis au premier angle, puis hors de l'outil. |
| Maintenir **Alt** | Suspend l'accroche tant que la touche est maintenue. |
| `S` | Active ou désactive l'accroche. |

## Astuces

- Pour aligner des blocs, commencez l'emprise sur l'angle d'un bloc existant : l'accroche aux extrémités le capte.
- Au-dessus de la barre d'outils, une consigne indique ce qu'attend l'étape en cours.

## Dépannage

**Les nombres saisis ne font rien.**
La vue 3D doit avoir le clavier : cliquez dedans, ou appuyez sur Tab jusqu'à ce qu'elle ait un contour vert, puis saisissez.

**L'outil Bloc est grisé.**
Les outils sont inactifs pendant le chargement de la vue 3D, quand elle n'est pas disponible sur cet ordinateur ou après son arrêt. Voir [Se déplacer dans la vue 3D](navigate-the-3d-view.fr.md#dépannage).

## Voir aussi

- [Sélectionner et déplacer des éléments](select-and-move-elements.fr.md)
- [Se déplacer dans la vue 3D](navigate-the-3d-view.fr.md)
