# Lire et saisir des coordonnées

Saisissez un point dans l’un des trois systèmes de coordonnées et lisez-le dans les deux autres : coordonnées du modèle, coordonnées cartographiques, latitude et longitude. Utile pour vérifier où se trouve un élément sur la carte, ou retrouver un point du géomètre dans votre modèle.

## Avant de commencer

- Le projet doit avoir une localisation. Sinon, le panneau le dit et propose **Définir la localisation du projet…**. Voir [Définir la localisation du projet](set-up-the-project-location.fr.md).

## Étapes

1. Affichez le panneau : choisissez l’onglet **Coordonnées** à droite de la fenêtre. **Raccourci :** Ctrl+Maj+K (Cmd+Maj+K sous macOS).
2. Le panneau s’ouvre sur l’origine du modèle, en trois blocs : **Modèle (local)** (x, y, z dans les axes du projet), **Carte** (Est, Nord et Hauteur dans le système du projet) et **Géographique** (latitude, longitude et hauteur sur le datum du système).
3. Saisissez une valeur dans n’importe quel champ d’un bloc. Ce bloc est marqué **Modifié** et les deux autres indiquent un instant plus tard **Calculé depuis** ce bloc. Appuyez sur Entrée pour convertir tout de suite.
4. Pour copier un bloc, choisissez **Copier** dans son titre. Les trois valeurs sont séparées par des tabulations et se collent dans les cellules d’un tableur.

**Résultat :** les trois blocs décrivent le même point. Les longueurs sont en mètres, au millimètre.

## Options

| Option | Effet |
| --- | --- |
| Degrés décimaux / DMS | Affiche latitude et longitude en degrés décimaux (huit décimales, environ 1 mm) ou en degrés, minutes et secondes. Vous pouvez saisir l’une ou l’autre forme. |
| Modifier… | Ouvre la boîte de dialogue de localisation du projet. |
| Revenir à l’origine | Revient à l’origine du modèle (0, 0, 0). |
| Choisir dans la vue | Réservé au choix d’un point à la souris dans une vue. Pas encore disponible. |

## Conseils

- Vous pouvez taper un point ou une virgule décimale. En DMS, écrivez par exemple `48°51′10,8″N`, ou `S 10 15 30`.
- La hauteur du bloc Géographique est approximative : BIMCraft ne modélise pas encore le géoïde (la forme du niveau moyen de la mer).

## Dépannage

**« Saisissez un nombre, p. ex. 652000,000. »**
Un champ contient un texte qui n’est pas un nombre. Corrigez-le et appuyez sur Entrée.

**« Ce point est trop éloigné de la projection pour être converti. »**
Le point est hors de la zone où le système de coordonnées fonctionne. Vérifiez les nombres ou le système de coordonnées du projet.

**Le panneau affiche « Ce projet n’a pas encore de localisation. »**
Choisissez **Définir la localisation du projet…** et suivez [Définir la localisation du projet](set-up-the-project-location.fr.md).

## Voir aussi

- [Définir la localisation du projet](set-up-the-project-location.fr.md)
