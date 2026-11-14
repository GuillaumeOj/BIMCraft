# Couper le modèle avec une boîte de coupe

Coupez une partie du modèle dans la vue 3D pour regarder à l'intérieur. Une boîte de coupe est une boîte autour de la partie à conserver : tout ce qui est en dehors est coupé, et les surfaces de coupe sont teintées pour être lisibles. Vous pouvez aussi couper avec un ou plusieurs plans de coupe. Couper ne modifie que l'affichage, jamais le modèle.

## Avant de commencer

- Le modèle contient au moins un élément, par exemple un [bloc](place-a-block.fr.md).
- La vue 3D fonctionne. Les réglages sont dans le panneau **Vue**, à droite.

Quelques mots employés sur cette page :

- **Boîte de coupe.** Une boîte dont les faces coupent le modèle. Ce qui est en dehors n'est pas dessiné et ne peut pas être sélectionné.
- **Plan de coupe.** Une surface plane infinie qui coupe tout ce qui se trouve d'un côté. Vous pouvez en ajouter six au maximum.
- **Face coupée.** La surface qui apparaît là où le modèle est coupé. Elle est teintée en orange pour ne jamais ressembler à une sélection.

## Activer la boîte de coupe

1. Ouvrez l'onglet **Vue** du panneau de droite.
2. Dans la section **Boîte de coupe**, cochez **Active**. **Raccourci :** `Alt+B`.

**Résultat :** une boîte qui entoure tout le modèle, avec une petite marge, apparaît avec une poignée sur chaque face. L'étiquette du panneau indique « Boîte de coupe active » et un message explique comment déplacer les faces.

## Déplacer les faces de la boîte

Il y a deux façons équivalentes de déplacer une face.

**En faisant glisser.** Faites glisser une poignée de la boîte. Maintenez **Maj** pour avancer par pas de 100 mm. La barre d'état affiche par exemple « Boîte de coupe · 1 face déplacée ».

**En saisissant.** Dans le panneau, les champs **Min** et **Max** donnent la position de chaque face selon X, Y et Z, en mètres.

1. Cliquez dans un champ, ou allez-y avec **Tab**.
2. Saisissez la nouvelle valeur en mètres, par exemple `12,5`.
3. Appuyez sur **Entrée** pour l'appliquer, ou sur **Échap** pour rétablir l'ancienne valeur. Quitter le champ avec **Tab** l'applique aussi.

**Résultat :** la face se déplace et la vue est coupée à sa nouvelle position.

Un minimum doit rester inférieur à son maximum. Sinon, le champ devient rouge et indique « Le minimum doit être inférieur au maximum » ; rien ne change tant que la valeur n'est pas corrigée.

## Autres réglages

| Option | Effet |
| --- | --- |
| **Faces coupées** | **Teintées** colore les surfaces de coupe ; **Aucune** les laisse ouvertes. |
| **Ajuster à la sélection** | Redimensionne la boîte autour des éléments sélectionnés. Sélectionnez d'abord quelque chose. |
| **Réinitialiser** | Remet la boîte autour de tout le modèle. |
| **Désactiver** | Désactive la boîte de coupe (`Alt+B`). |

## Couper avec des plans de coupe

1. Dans la section **Plans de coupe**, choisissez **Ajouter sur une face…**. Un message indique « Cliquez sur une face pour couper à cet endroit · Échap pour annuler ».
2. Cliquez sur une face du modèle. Le plan passe par ce point, tourné vers vous : ce qui se trouve entre vous et la face est coupé.

**Résultat :** le panneau liste « Plan 1 », avec sa direction (par exemple « Normale +X ») et sa position en mètres. La barre d'état indique combien de plans sont actifs.

Pour chaque plan de la liste :

- **Inverser** change le côté qui est coupé.
- **Supprimer** supprime le plan. Quand le focus est sur les boutons d'un plan, **Suppr** le supprime aussi.
- **Tout supprimer** retire tous les plans. **Annuler** ne peut pas revenir dessus : les plans font partie de la vue, pas du modèle.

Vous pouvez avoir six plans au maximum ; à six, **Ajouter sur une face…** indique « 6 plans au maximum ».

## Astuces

- Utilisez les vues **Dessus**, **Avant** et **Gauche** des commandes de vue pour lire les coupes comme des plans et des élévations.
- La boîte de coupe et les plans ne sont conservés que tant que la fenêtre est ouverte. Ils ne sont pas encore enregistrés avec le projet.
- Les éléments masqués restent masqués dans une coupe ; voir [Masquer et isoler des éléments](hide-and-isolate-elements.fr.md).

## Dépannage

**La boîte de coupe est active mais le modèle ne change pas.**
La boîte est plus grande que le modèle ou a gardé sa taille de départ. Choisissez **Réinitialiser**, puis rentrez une face.

**Je ne peux pas sélectionner un élément.**
Il est coupé. Les éléments en dehors de la boîte ou derrière un plan ne sont pas dessinés et ne peuvent pas être sélectionnés. Déplacez la face ou supprimez le plan.

**Un champ devient rouge.**
La valeur n'est pas un nombre, ou le minimum n'est pas inférieur au maximum. Lisez le message sous les champs, corrigez la valeur et appuyez sur **Entrée**.

**« Ajuster à la sélection » ne fait rien.**
Il faut une sélection. Sélectionnez d'abord un élément dans la vue 3D.

**Une surface de coupe montre des taches teintées.**
Les surfaces de certains éléments importés ne sont pas parfaitement fermées. La teinte n'est alors qu'approximative ; la coupe, elle, est correcte.

## Voir aussi

- [Masquer et isoler des éléments](hide-and-isolate-elements.fr.md)
- [Se déplacer dans la vue 3D](navigate-the-3d-view.fr.md)
