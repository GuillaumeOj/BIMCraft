# Créer une composition de mur ou de plancher

Une composition liste les couches d'un mur ou d'un plancher, d'une face à l'autre : par exemple une finition, une couche d'isolant, un noyau en béton et une plaque de plâtre. Construisez la composition une fois, avec ses matériaux et ses épaisseurs, et réutilisez-la.

## Avant de commencer

- Les matériaux des couches doivent être dans votre projet. Vous pouvez les ajouter pendant que vous construisez la composition (voir [Gérer les matériaux du projet](manage-project-materials.fr.md)).

Quelques mots employés sur cette page :

- **Couche.** Une bande de matériau d'une épaisseur donnée. Une composition a de 1 à 32 couches.
- **Fonction.** Le rôle de la couche : structure, support, isolant, lame d'air, membrane ou finition.
- **Noyau.** La partie porteuse de la composition. Les couches du noyau doivent être côte à côte.
- **Priorité de jonction.** Quand deux murs se rencontrent, les couches de priorité plus haute (1) traversent celles de priorité plus basse (5) ; les couches de même priorité se joignent. Chaque fonction a une valeur par défaut : structure 1, support 2, isolant 3, lame d'air 3, membrane 4, finition 5.
- **Résistance thermique (R).** La capacité d'une couche, ou de toute la composition, à s'opposer au passage de la chaleur, en m²·K/W. BIMCraft la calcule à partir de l'épaisseur et de la conductivité thermique de chaque matériau.

## Étapes

1. Choisissez **Compositions…** dans la barre de titre. **Raccourci :** Ctrl+Maj+B (Cmd+Maj+B sur macOS). Le raccourci ne fait rien tant qu'une autre boîte de dialogue est ouverte.
2. Choisissez **Nouvelle composition…** puis **Nouvelle composition de mur** ou **Nouvelle composition de plancher**. (Dans un projet vide, choisissez directement l'un des deux boutons.) Une composition de mur liste ses couches de l'extérieur vers l'intérieur ; une composition de plancher du dessus vers le dessous. Vous ne pourrez plus le changer ensuite.
3. Saisissez un **Nom**, et une **Description** si vous le souhaitez.
4. Remplissez la première couche : choisissez sa **Fonction** et son **Matériau**, et saisissez son **Épaisseur** en millimètres. Le point décimal et la virgule décimale fonctionnent tous deux. Seule une membrane peut faire 0 mm.
5. Choisissez **Ajouter une couche** pour chaque couche suivante (ou appuyez sur Ctrl+Entrée dans une couche : la nouvelle couche se place après elle).
6. Facultatif : ajustez la **Priorité de jonction** ou la case **Noyau** d'une couche. Une priorité modifiée porte la mention **Modifiée** ; choisissez dans la liste l'option marquée « (par défaut) » pour y revenir.
7. Choisissez **Appliquer**. **Raccourci :** Entrée dans un champ de texte.

**Résultat :** la composition apparaît dans la liste, avec un message « Composition appliquée » et un bouton **Annuler**. Appliquer est une seule étape d'annulation.

## Vérifier la section et les totaux

Pendant que vous modifiez, la boîte de dialogue montre un **Aperçu de la section** (schématique, pas à l'échelle), l'**Épaisseur totale** et la **Résistance thermique**. BIMCraft demande ces valeurs à son noyau de calcul : ce sont toujours les valeurs réelles.

Si une couche a un matériau sans conductivité thermique, comme une lame d'air, sa résistance affiche « — ». Le total compte les autres couches et une note indique combien de couches ont été laissées de côté. Les résistances superficielles et le coefficient U ne font pas partie de ce total.

## Réordonner, ajouter et supprimer des couches

- **Monter la couche …** et **Descendre la couche …** déplacent une couche d'un cran. Au clavier, appuyez sur **Alt+↑** ou **Alt+↓** sur un bouton de la couche ou sur sa case Noyau. Le focus reste sur la commande utilisée, et la nouvelle position est lue par les lecteurs d'écran.
- **Supprimer la couche …** retire une couche. Une composition garde toujours au moins une couche.
- Une composition peut avoir au plus 32 couches.

## Choisir ou ajouter un matériau

Dans la liste **Matériau**, choisissez un matériau de votre projet. Le bouton **Gérer les matériaux…** à côté de la liste ouvre la boîte de dialogue Matériaux par-dessus l'éditeur : ajoutez ou créez un matériau, appuyez sur **Terminé**, et le nouveau matériau est choisi pour la couche. Une couche de **lame d'air** ne peut utiliser que des matériaux génériques.

## Modifier, dupliquer, renommer et supprimer une composition

Dans la liste, ouvrez le menu **…** d'une composition :

| Option | Effet |
| --- | --- |
| Modifier… | Ouvre l'éditeur. Entrée ou un double clic sur la ligne fait de même. |
| Dupliquer… | Crée une copie sous le nom que vous choisissez. |
| Renommer… | Change le nom dans la ligne. F2 fait de même. |
| Supprimer… | Supprime la composition et ses couches après confirmation. Suppr fait de même. Vous pouvez annuler. |

Utilisez **Murs**, **Planchers** ou **Toutes** pour filtrer la liste.

## Abandonner vos modifications

Choisissez **Annuler** ou appuyez sur Échap. Si vous avez modifié quelque chose, BIMCraft demande « Abandonner vos modifications ? » : **Continuer la modification** est sélectionné par défaut.

## Dépannage

**« Choisissez un matériau. » sous une couche.**
Chaque couche a besoin d'un matériau. Choisissez-en un dans la liste.

**« Saisissez une épaisseur supérieure à 0 mm. »**
Une couche a besoin d'une épaisseur. Seule une membrane peut faire 0 mm.

**« Les couches du noyau doivent être côte à côte. »**
Une couche qui n'est pas dans le noyau se trouve entre des couches marquées Noyau. Cochez ou décochez les cases Noyau pour que le noyau forme un seul bloc.

**« Une composition porte déjà ce nom. »**
Choisissez un autre nom ; les majuscules ne comptent pas.

**« Ce matériau ne peut pas être utilisé pour une lame d'air. »**
Une lame d'air prend un matériau générique. Choisissez-en un, ou changez la fonction de la couche.

**« Cette composition a changé depuis son ouverture. »**
Un autre changement a touché la composition pendant que vous la modifiiez. Fermez-la et rouvrez-la.

## Voir aussi

- [Gérer les matériaux du projet](manage-project-materials.fr.md)
