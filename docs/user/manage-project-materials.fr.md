# Gérer les matériaux du projet

Chaque mur, dalle, poutre ou poteau est fait d'un matériau, par exemple une classe de béton ou une nuance d'acier. Dans la boîte de dialogue **Matériaux**, vous ajoutez à votre projet des matériaux issus d'une bibliothèque de classes européennes courantes, vous créez les vôtres, vous vérifiez ou modifiez leurs valeurs, et vous retirez ceux dont vous n'avez plus besoin.

## Avant de commencer

- Rien : un projet démarre sans matériau. Vous ajoutez ceux dont vous avez besoin.

Quelques mots employés sur cette page :

- **Classe.** Une catégorie normalisée de matériau, comme le béton C25/30 ou l'acier S355. La bibliothèque contient des classes ; votre projet contient des *matériaux*, qui sont des copies de classes (ou des matériaux que vous avez créés).
- **Loi de comportement.** La manière dont la contrainte suit la déformation dans le matériau, par exemple *élastique linéaire* ou *parabole-rectangle* (béton comprimé). Elle vient avec la classe ; elle est affichée, pas modifiable.
- **Source.** La norme et l'article ou le tableau dont provient une valeur, par exemple « EN 1992-1-1:2004 · Tableau 3.1 ». Les notes de calcul citent ces sources, jamais le texte de la norme.

## Ouvrir la boîte de dialogue Matériaux

Choisissez **Matériaux…** dans la barre de titre. **Raccourci :** Ctrl+Maj+M (Cmd+Maj+M sur macOS). Le raccourci ne fait rien tant qu'une autre boîte de dialogue est ouverte.

La liste des matériaux du projet est à gauche et le matériau sélectionné à droite. Appuyez sur **Terminé** ou sur Échap pour fermer la boîte de dialogue ; chaque modification est déjà enregistrée.

## Ajouter une classe depuis la bibliothèque

1. Dans la boîte de dialogue Matériaux, choisissez **Ajouter depuis la bibliothèque…**. Un projet sans matériau ouvre directement la bibliothèque.
2. Saisissez dans **Rechercher dans la bibliothèque** (un nom comme `C30`), ou choisissez une catégorie sous le champ de recherche. Les flèches haut et bas parcourent les classes.
3. Sélectionnez une classe pour voir ses valeurs et leurs sources à droite. Une classe déjà présente dans votre projet porte la mention « Dans le projet ».
4. Facultatif : modifiez **Nom dans ce projet**. Les valeurs sont copiées dans votre projet ; les mises à jour ultérieures de la bibliothèque ne modifient jamais vos matériaux. Deux matériaux ne peuvent pas porter le même nom (les majuscules ne comptent pas).
5. Choisissez **Ajouter au projet** (ou appuyez sur Entrée dans le champ du nom).

**Résultat :** le matériau est ajouté et sélectionné, et un message comme « C25/30 ajouté » apparaît avec un bouton **Annuler**.

## Créer votre propre matériau

1. Choisissez **Nouveau matériau…**.
2. Saisissez un **Nom** (de 1 à 120 caractères) et choisissez une **Catégorie**.
3. Choisissez **Créer**.

**Résultat :** le matériau s'ouvre avec toutes ses valeurs vides. Utilisez **Ajouter une valeur** sur une ligne pour saisir ce que vous connaissez. Les valeurs saisies portent la mention **Saisie par l'utilisateur**.

## Lire et modifier une valeur

1. Sélectionnez un matériau dans la liste. Les onglets **Mécanique** et **Thermique** donnent accès à ses propriétés. Les onglets Graphisme et Environnement sont affichés pour annoncer ce qui vient ; ils ne sont pas encore disponibles.
2. Chaque ligne montre la propriété, sa valeur et sa source. Une propriété est toujours affichée dans la même unité : mégapascals (MPa) pour les résistances, gigapascals (GPa) pour les modules, kg/m³ pour la masse volumique, W/(m·K) pour la conductivité thermique et ‰ pour les déformations. Un coefficient partiel a une ligne par annexe nationale (Recommandée et France).
3. Pour modifier une valeur, choisissez-la (ou appuyez sur Entrée ou F2 dessus). Saisissez le nouveau nombre : le point décimal et la virgule décimale fonctionnent tous deux. Appuyez sur Entrée ou quittez le champ pour l'enregistrer ; Échap annule.

**Résultat :** la ligne affiche le badge **Saisie par l'utilisateur** avec un filet coloré, et la modification est une étape que vous pouvez annuler.

Si une valeur n'est pas acceptée, un message sous le champ explique pourquoi et, quand elle est connue, la plage autorisée, par exemple « Entre 0 et 0,5 ». Le champ reste ouvert pour que vous corrigiez.

## Revenir à la valeur de la bibliothèque

Sur une valeur modifiée, choisissez **Rétablir la valeur de la bibliothèque**. Pointez le bouton pour voir d'abord la valeur de la bibliothèque. Le badge disparaît et la source revient.

Un matériau que vous avez créé n'a pas de valeur de bibliothèque : videz le champ pour supprimer une valeur. Une valeur obligatoire, comme la résistance d'un béton, ne peut pas être vidée.

## Renommer ou supprimer un matériau

1. Sélectionnez le matériau, puis ouvrez le menu **…** à côté de son nom et choisissez **Renommer…** ou **Supprimer…**. Depuis la liste, F2 renomme et Suppr supprime.
2. Pour renommer, saisissez le nouveau nom et appuyez sur Entrée (Échap annule).
3. Pour supprimer, confirmez dans la barre du bas : **Conserver le matériau** est sélectionné par défaut ; choisissez **Supprimer le matériau** pour le retirer. Un message apparaît avec un bouton **Annuler**.

Un matériau utilisé par des couches de compositions ne peut pas être supprimé. La boîte de dialogue indique combien de couches l'utilisent : changez d'abord leur matériau, puis supprimez-le.

## Options

| Option | Effet |
| --- | --- |
| Rechercher des matériaux | Affiche les matériaux dont le nom contient le texte. |
| Catégorie | Affiche une seule catégorie de matériaux. |
| Ajouter depuis la bibliothèque… | Ajoute une classe de la bibliothèque au projet. |
| Nouveau matériau… | Crée un matériau de votre choix. |
| Rétablir la valeur de la bibliothèque | Remet une valeur modifiée à celle de la classe, avec sa source. |

## Les classes de la bibliothèque

| Matériau | Classes | Valeurs selon |
| --- | --- | --- |
| Béton | C12/15, C16/20, C20/25, C25/30, C30/37, C35/45, C40/50, C45/55, C50/60 | EN 1992-1-1:2004 (Eurocode 2) |
| Acier d'armature | B500A, B500B, B500C | EN 1992-1-1:2004, annexe C |
| Acier de construction | S235, S275, S355, S420, S460 | EN 1993-1-1:2005 (Eurocode 3), pour des éléments jusqu'à 40 mm d'épaisseur |
| Bois | C16 à C35, GL24h, GL28h, GL32h | EN 338:2016 et EN 14080:2013 |
| Maçonnerie | Trois maçonneries courantes, en terre cuite et en blocs de béton | EN 1996-1-1:2005 |
| Autres | Isolants, finitions et matériaux génériques | Règles Th-Bât, Fascicule Matériaux (2017) |

Une classe de béton comme C30/37 donne sa résistance caractéristique sur cylindre, puis sur cube, en mégapascals (MPa). Les valeurs de la bibliothèque sont des valeurs *caractéristiques*. Les coefficients partiels de sécurité viennent avec la classe pour chaque annexe nationale ; vérifiez-les, comme toutes les valeurs, avec votre exemplaire des normes et l'annexe nationale applicable à votre projet.

## Ajouter des classes depuis un fichier de bibliothèque

Pour ajouter beaucoup de classes à la fois, ou les partager avec une équipe, vous pouvez ajouter depuis un fichier de bibliothèque des classes absentes de la bibliothèque, par exemple un produit issu de la fiche technique d'un fabricant. Un fichier de bibliothèque liste des matériaux sous la même forme que ceux fournis. Chaque valeur est en unités SI (pascals pour les résistances et modules, kilogrammes par mètre cube pour la masse volumique, nombres sans unité pour les déformations et rapports) et indique son document et son article :

```json
{
  "format": 1,
  "materials": [
    {
      "name": "Panneau fournisseur",
      "category": "generic",
      "behaviour_law": "linear_elastic",
      "properties": {
        "e_modulus": { "value": 4.0e9, "document": "Fiche technique fournisseur 2025", "clause": "Tableau 2" }
      }
    }
  ]
}
```

**Résultat :** les nouvelles classes apparaissent à côté de celles fournies dans la bibliothèque. Un fichier est ajouté en entier ou pas du tout : si un seul de ses matériaux est faux, aucun n'est ajouté.

## Conseils

- Utilisez le nom courant de la profession (C30/37, S355), pour que chacun reconnaisse la classe.
- Indiquez l'édition de la norme dans le document (« EN 1992-1-1:2004 ») : les numéros d'articles changent d'une édition à l'autre.
- Annuler (Ctrl+Z, Cmd+Z sur macOS) reprend une modification, un renommage, un ajout ou une suppression.

## Dépannage

**« Un matériau porte déjà ce nom. »**
Les noms doivent être différents dans un projet, quelles que soient les majuscules. Choisissez un autre nom, par exemple « C30/37 (dalle) ».

**« Saisissez une valeur valide pour cette propriété. »**
Le texte n'est pas un nombre, ou la valeur est en dehors de ce que le matériau accepte. Le message indique la plage autorisée quand il y en a une. Saisissez la valeur dans l'unité affichée dans le champ.

**« Cette valeur est obligatoire. »**
Une résistance, comme celle d'un béton, doit avoir une valeur. Saisissez un nombre au lieu de vider le champ.

**Supprimer… indique que des couches de compositions utilisent le matériau.**
Ouvrez les compositions qui l'utilisent, choisissez un autre matériau pour ces couches, puis supprimez celui-ci.

**Les onglets Graphisme et Environnement ne réagissent pas.**
Ils ne sont pas encore disponibles.

**« material #… has the key of another material »**
Le fichier ajoute une classe qui existe déjà, ou en liste une deux fois. Renommez-la, par exemple « C30/37 (projet) ».

**« material #… is not valid: the material needs the property … »**
La loi de comportement, ou la catégorie, a besoin d'une propriété que le matériau ne donne pas. Un matériau élastique linéaire a besoin de son module d'Young ; un béton de sa résistance, de ses déformations au pic et ultime et de l'exposant de la parabole ; un acier de sa limite d'élasticité et de son module d'Young.

**« material #… is not valid: the property … is out of range »**
Une valeur est négative, nulle ou non physique, par exemple un coefficient de Poisson de 0,5 ou plus. Vérifiez son unité : les résistances sont en pascals, 30 MPa s'écrit donc 30e6.

**« material #…: the source of … is not valid »**
Une valeur n'a pas de document ou pas d'article. Chaque valeur a besoin des deux.

**« the library source is not valid at line …, column … »**
Le fichier n'est pas un JSON valide, ou utilise une clé que BIMCraft ne connaît pas. Vérifiez l'endroit indiqué par le message.

## Voir aussi

- [Enregistrer et ouvrir un projet](save-and-open-a-project.md) (en anglais)
