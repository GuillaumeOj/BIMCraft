# Définir la localisation du projet

Placez votre projet sur la Terre : choisissez le système cartographique de votre relevé, indiquez où se trouve l’origine de votre modèle sur cette carte et comment votre bâtiment est orienté. Faites-le une fois, au début du projet, pour pouvoir ensuite lire et donner des coordonnées cartographiques.

## Avant de commencer

- Ayez sous la main votre relevé ou votre plan cadastral. Il vous faut le nom de son **système de coordonnées** (par exemple « Lambert 93 » ou « CC48 ») et les coordonnées du point que vous voulez utiliser comme origine du modèle.

Quelques termes employés sur cette page :

- **Système de coordonnées (SCR).** Le système cartographique qui donne à chaque lieu deux nombres, en mètres. En France, le Lambert 93 et les zones CC (CC42 à CC50) sont courants. Chacun a un code numérique (EPSG 2154 pour le Lambert 93).
- **Point de base.** Les coordonnées sur la carte de l’origine de votre modèle, le point (0, 0, 0). L’Est est la distance vers l’est, le Nord la distance vers le nord, la Hauteur l’altitude.
- **Rotation de la grille.** L’angle entre la grille de la carte et celle de votre projet, dans le sens trigonométrique. Laissez 0 si les axes de votre bâtiment suivent la carte.
- **Nord de la grille et nord géographique.** Le nord de la grille est la direction des lignes verticales de la carte. Le nord géographique pointe vers le pôle Nord. Ils diffèrent légèrement, et l’écart augmente en s’éloignant du centre de la zone. BIMCraft calcule l’angle du nord géographique et dessine côte à côte le nord du projet, le nord de la grille et le nord géographique.
- **Point de repère topographique.** Un repère dont vous connaissez les coordonnées, comme un piquet ou un angle de parcelle. Une fois saisi, BIMCraft montre sa position dans votre modèle.

## Étapes

1. Ouvrez la boîte de dialogue : choisissez **Localisation du projet…** dans la barre de titre. **Raccourci :** Ctrl+Maj+L (Cmd+Maj+L sous macOS).
2. Dans **1 Système de coordonnées**, saisissez un code ou une partie du nom, par exemple `2154` ou `Lambert`. Choisissez dans la liste avec les flèches et Entrée, ou à la souris. Sa zone d’usage s’affiche sous le champ.
3. Dans **2 Point de base du projet**, saisissez l’Est, le Nord et la Hauteur de l’origine de votre modèle, en mètres. Vous pouvez taper un point ou une virgule décimale.
4. Dans **3 Orientation**, saisissez la **Rotation de la grille**. Choisissez **Degrés décimaux** ou **DMS** (degrés, minutes, secondes).
5. Facultatif : ouvrez **Avancé** pour modifier l’**Échelle**. Laissez 1 sauf si votre géomètre vous a donné une échelle de projet.
6. Facultatif : ouvrez **Point de repère topographique (facultatif)** et saisissez l’Est, le Nord et la Hauteur de votre repère. Renseignez les trois valeurs, ou aucune.
7. Choisissez **Appliquer** (ou appuyez sur Entrée).

**Résultat :** la boîte de dialogue se ferme et un message confirme « Localisation du projet appliquée ».

Quand vous rouvrez la boîte de dialogue, elle montre ce que vous avez saisi et les informations calculées par BIMCraft : l’angle du nord géographique, la latitude et la longitude du point de base, la convergence des méridiens (l’angle entre le nord de la grille et le nord géographique), le coefficient d’échelle au point et la position du repère dans le modèle.

## Options

| Option | Effet |
| --- | --- |
| Système de coordonnées | Le système cartographique de votre relevé. Seuls les systèmes en mètres sont proposés. |
| Est, Nord, Hauteur | Coordonnées de l’origine du modèle sur la carte, en mètres, au millimètre. |
| Rotation de la grille | Angle entre la grille de la carte et celle du projet, dans le sens trigonométrique, en degrés. |
| Échelle | Échelle du projet, entre 0,000001 et 1 000 000. En principe 1. |
| Point de repère topographique | Un repère connu sur la carte, facultatif. |
| Supprimer la géoréférence | Oublie la position du projet sur la Terre. Le modèle n’est pas modifié. Demande une confirmation et peut être annulé. |

## Conseils

- Le bouton **i** à côté d’un champ donne une courte explication.
- Échap ferme la boîte de dialogue sans appliquer. Dans la liste des systèmes, Échap ferme d’abord la liste.
- Choisissez **Appliquer** quand rien n’a changé pour simplement fermer la boîte de dialogue.

## Dépannage

**« Choisissez un système de coordonnées. »**
Choisissez un système dans la liste. Taper son nom ne suffit pas : cliquez sur la ligne voulue.

**« Ce point est hors de la zone de validité de ce système de coordonnées. »**
L’Est et le Nord ne tombent pas dans la zone du système choisi. Vérifiez le système (par exemple la bonne zone CC) et que vous n’avez pas inversé l’Est et le Nord.

**« L’échelle doit être comprise entre 0,000001 et 1000000. »**
Saisissez une échelle proche de 1, comme `1` ou `0,9996`.

**« Ce projet contient plusieurs sites. »**
BIMCraft ne peut placer sur la Terre qu’un projet ayant un seul site. Supprimez d’abord les sites en trop.

**Je ne trouve pas mon système de coordonnées.**
Seuls les systèmes intégrés à BIMCraft sont proposés, et uniquement ceux en mètres. Les systèmes en latitude et longitude ne servent pas à placer un projet.

## Voir aussi

- [Lire et saisir des coordonnées](read-and-enter-coordinates.fr.md)
- [Utiliser l’espace de travail au clavier](use-the-workspace-with-the-keyboard.md)
