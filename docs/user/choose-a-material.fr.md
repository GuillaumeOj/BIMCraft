# Choisir un matériau

Chaque mur, dalle, poutre ou poteau est fait d'un matériau, par exemple une classe de béton ou une nuance d'acier. BIMCraft fournit une bibliothèque des classes européennes courantes, chacune avec les valeurs dont un calcul de structure a besoin, et vous permet d'ajouter les vôtres.

## Avant de commencer

- BIMCraft n'a pas encore de version publiée. Le sélecteur de matériaux, ses entrées de menu et ses noms dans la palette de commandes seront ajoutés à cette page avec la première version qui l'affiche dans l'interface.

## Les classes de la bibliothèque

| Matériau | Classes | Valeurs selon |
| --- | --- | --- |
| Béton | C20/25, C25/30, C30/37, C35/45, C40/50, C45/55, C50/60 | EN 1992-1-1:2004 (Eurocode 2) |
| Acier d'armature | B500B | EN 1992-1-1:2004, annexe C |
| Acier de construction | S235, S275, S355, S420, S460 | EN 1993-1-1:2005 (Eurocode 3), pour des éléments jusqu'à 40 mm d'épaisseur |

Une classe de béton comme C30/37 donne sa résistance caractéristique sur cylindre, puis sur cube, en mégapascals (MPa).

Chaque valeur est accompagnée de sa **source** : la norme et l'article ou le tableau dont elle provient, par exemple « EN 1992-1-1:2004, Table 3.1 ». Les notes de calcul citent ces sources, jamais le texte de la norme. Vérifiez les valeurs avec votre exemplaire des normes et l'annexe nationale applicable à votre projet.

## Ce que contient un matériau

- **Sa catégorie** : béton, armature, acier, bois, maçonnerie, isolant, finition ou générique.
- **Sa loi de comportement**, qui dit comment la contrainte suit la déformation :
  - *Élastique linéaire* : la contrainte croît proportionnellement à la déformation, en traction comme en compression.
  - *Parabole-rectangle* pour le béton comprimé : une courbe jusqu'à la déformation au pic de contrainte, puis une contrainte constante jusqu'à la déformation ultime. Le béton ne reprend pas de traction.
  - *Bilinéaire* pour l'acier : élastique jusqu'à la limite d'élasticité, puis une branche légèrement montante jusqu'à la déformation ultime (acier d'armature) ou horizontale (acier de construction).
- **Ses propriétés** : résistances, module d'élasticité, coefficient de Poisson, dilatation thermique et masse volumique, chacune avec sa source.

Les valeurs de la bibliothèque sont des valeurs *caractéristiques*. Les coefficients partiels de sécurité ne font pas partie du matériau : ils viennent du code de calcul et de l'annexe nationale de votre projet.

## Ajouter vos propres classes

Vous pouvez ajouter des classes absentes de la bibliothèque, par exemple une classe de bois ou un produit issu de la fiche technique d'un fabricant. Un fichier de bibliothèque liste des matériaux sous la même forme que ceux fournis. Chaque valeur est en unités SI (pascals pour les résistances et modules, kilogrammes par mètre cube pour la masse volumique, nombres sans unité pour les déformations et rapports) et indique son document et son article :

```json
{
  "format": 1,
  "materials": [
    {
      "name": "GL24h",
      "category": "other",
      "behaviour_law": "linear_elastic",
      "design_code": "EN 14080:2013",
      "properties": {
        "YoungModulus": { "value": 11.5e9, "document": "EN 14080:2013", "clause": "Table 5" }
      }
    }
  ]
}
```

**Résultat :** les nouvelles classes apparaissent à côté de celles fournies. Un fichier est ajouté en entier ou pas du tout : si un seul de ses matériaux est faux, aucun n'est ajouté.

## Conseils

- Utilisez le nom courant de la profession (C30/37, S355), pour que chacun reconnaisse la classe.
- Indiquez l'édition de la norme dans le document (« EN 1992-1-1:2004 ») : les numéros d'articles changent d'une édition à l'autre.

## Dépannage

**« material #… has the name of another material »**
Le fichier ajoute une classe qui existe déjà, ou en liste une deux fois. Renommez-la, par exemple « C30/37 (projet) ».

**« material #… is not valid: the behaviour law needs the property … »**
La loi de comportement lit une propriété que le matériau ne donne pas. Un matériau élastique linéaire a besoin de son module d'Young ; un béton de sa résistance, de ses déformations au pic et ultime et de l'exposant de la parabole ; un acier de sa limite d'élasticité et de son module d'Young.

**« material #… is not valid: the property … is out of range »**
Une valeur est négative, nulle ou non physique, par exemple un coefficient de Poisson de 0,5 ou plus. Vérifiez son unité : les résistances sont en pascals, 30 MPa s'écrit donc 30e6.

**« material #…: the source of … is not valid »**
Une valeur n'a pas de document ou pas d'article. Chaque valeur a besoin des deux.

**« the library file is not valid at line …, column … »**
Le fichier n'est pas un JSON valide, ou utilise une clé que BIMCraft ne connaît pas. Vérifiez l'endroit indiqué par le message.

## Voir aussi

- [Enregistrer et ouvrir un projet](../save-and-open-a-project.md) (en anglais)
