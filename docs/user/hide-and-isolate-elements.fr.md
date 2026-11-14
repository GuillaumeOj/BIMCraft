# Masquer et isoler des éléments

Masquez les éléments inutiles ou isolez ceux que vous voulez examiner, par catégorie, par discipline ou par sélection. Masquer et isoler ne changent que l'affichage de la vue 3D ; le modèle n'est jamais modifié, et ces réglages sont oubliés à la fermeture de la fenêtre.

## Avant de commencer

- Le modèle contient au moins un élément, par exemple un [bloc](place-a-block.fr.md).

Quelques mots employés sur cette page :

- **Catégorie.** Le type d'élément : murs, dalles, poteaux…
- **Discipline.** Le corps de métier auquel appartient un élément : architecture, structure…
- **Isoler.** N'afficher que certains éléments et masquer tous les autres.

## Masquer ou afficher une catégorie ou une discipline

1. Dans les commandes de vue, en bas à droite, choisissez **Visibilité** (l'œil). La liste de visibilité s'ouvre, première ligne sélectionnée.
2. Choisissez **Catégorie** ou **Discipline** en haut.
3. Cliquez sur une ligne pour la masquer, et cliquez à nouveau pour l'afficher. Au clavier, déplacez-vous avec **↑**, **↓**, **Début** et **Fin**, et appuyez sur **Espace** pour masquer ou afficher.

**Résultat :** la ligne devient grise et indique « Masqué », et ses éléments disparaissent de la vue. Les éléments masqués ne peuvent ni être désignés ni servir d'accroche. Le panneau Vue les liste sous **Visibilité**.

Appuyez sur **Échap** pour fermer la liste ; le focus revient sur le bouton en forme d'œil. Vous pouvez aussi cliquer en dehors.

## Isoler

**Une catégorie ou une discipline.** Dans la liste de visibilité, appuyez sur **Maj+Espace** sur une ligne, ou cliquez dessus avec Maj. La ligne est marquée « Isolé » et tout le reste est masqué. Faites-le sur une deuxième ligne pour afficher les deux, et sur une ligne isolée pour la retirer.

**La sélection.** Sélectionnez des éléments, puis choisissez **Isoler la sélection** dans la liste, ou appuyez sur `I`.

**Résultat :** un message indique par exemple « Isoler · seul Structure est affiché · Échap ou « Terminer l’isolement » pour revenir ». Le panneau Vue porte l'étiquette « Isolé ».

Si vous supprimez tous les éléments isolés (ou tous ceux de la catégorie ou de la discipline isolée), l'isolement se termine de lui-même et les autres éléments réapparaissent ; ceux que vous avez masqués restent masqués. Annuler la suppression ne les isole pas de nouveau.

## Masquer la sélection

Sélectionnez des éléments, puis choisissez **Masquer** dans la liste, ou appuyez sur `H`. Les éléments sont masqués, et des éléments masqués ne peuvent pas être sélectionnés.

## Tout remettre

| Quoi | Comment |
| --- | --- |
| Terminer un isolement | **Terminer l’isolement** dans le panneau Vue, ou **Échap** (partout où se trouve le focus, sauf dans un champ de texte). |
| Tout afficher | **Réinitialiser la visibilité** dans le panneau Vue, ou `Alt+H`. Cela efface les catégories, disciplines et sélections masquées, et tout isolement. |

## Raccourcis clavier

| Raccourci | Effet |
| --- | --- |
| `H` | Masquer la sélection. |
| `I` | Isoler la sélection. |
| `Alt+H` | Réinitialiser la visibilité. |
| **Échap** | Terminer un isolement. |

`H` et `I` sont des raccourcis à une seule touche. Vous pouvez les désactiver avec **Raccourcis à une touche** dans la barre de titre ; ils ne fonctionnent alors que lorsque la vue 3D a le focus. `Alt+H` n'est pas concerné.

## Dépannage

**« Sélectionnez d’abord ce qu’il faut masquer ou isoler ».**
`H`, `I` et les deux boutons agissent sur la sélection, et rien n'est sélectionné. Sélectionnez d'abord un élément.

**Un élément manque dans la vue.**
Il est peut-être masqué, hors de la boîte de coupe ou derrière un plan de coupe. Regardez la section **Visibilité** du panneau Vue, puis [Couper le modèle avec une boîte de coupe](cut-the-model-with-a-section-box.fr.md).

**La liste de visibilité est vide.**
Le modèle ne contient pas encore d'éléments de ce type. La liste reprend ce que contient le modèle.

**Appuyer sur `H` ou `I` écrit une lettre.**
Le focus est dans un champ de texte, où les lettres sont du texte. Appuyez sur **Échap** ou cliquez dans la vue 3D, puis réessayez.

## Voir aussi

- [Couper le modèle avec une boîte de coupe](cut-the-model-with-a-section-box.fr.md)
- [Sélectionner et déplacer des éléments](select-and-move-elements.fr.md)
