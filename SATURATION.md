# Ritmo-6 — saturation des coups rapprochés (version 1.3.4)

## Ce que font les vraies TR-808 et TR-909

Sur ces machines, rien n'est « échantillonné » : chaque voix est un petit circuit analogique qui reçoit
une impulsion de déclenchement (environ 1 ms) et répond par un son qui s'éteint lentement. Quand on la
redéclenche **avant qu'elle ait fini de sonner**, trois choses se produisent :

1. **L'énergie s'additionne.** Le cœur de la grosse caisse 808 est un filtre résonant (un « pont en T »)
   qui sonne comme une cloche frappée. Un nouveau coup ajoute son énergie à ce qui vibre encore ; selon le
   moment où il tombe, la vibration est renforcée (le plus souvent) ou en partie annulée. Les enveloppes à
   condensateur (caisse claire, toms, charleston de la 909, VCA des cymbales de la 808) repartent du niveau
   où elles étaient : le son ne redescend jamais à zéro entre deux coups rapides, il devient plus dense.
2. **L'étage de sortie sature.** Le signal qui grossit atteint la limite des transistors et amplis
   opérationnels de sortie : les crêtes s'arrondissent, le son s'épaissit et gagne des harmoniques
   impaires. C'est doux, pas un écrêtage brutal.
3. **Cela dépend du temps qui passe.** Un coup isolé, ou des coups espacés quand la voix a eu le temps de
   s'éteindre, sont toujours propres. Plus les coups se rapprochent, et plus le son est long (une grosse
   caisse longue plus qu'un charleston court), plus l'effet monte. Il est donc **graduel**.

*Sources : la description des circuits de la TR-808 et de la TR-909 (manuels de service Roland et les
analyses publiées du circuit de grosse caisse 808, notamment le modèle de K. J. Werner, J. S. Abel et
J. O. Smith, DAFx 2014). La recherche web depuis l'outil n'a pas donné de page lisible ; ce résumé
s'appuie sur ces analyses connues. Les chiffres ci-dessous sont un modèle, pas une mesure d'une machine
réelle.*

## Ce que faisait Ritmo-6 avant (1.1 à 1.3.3)

- Seuls les coups **MULTIPLY** chargeaient le « condensateur » ; un pas normal le remettait à 1, même si les
  pas se suivaient vite.
- Dès le 2ᵉ coup d'un pas MULTIPLY, la charge passait de 1 à environ 2 et la saturation était **complète
  d'un coup** (mélange à 100 %) : d'où l'impression que « ça sature dès qu'il y a un pas MULTIPLY ».

## Ce que fait Ritmo-6 maintenant (1.3.4)

- **Chaque coup** (pas normal ou MULTIPLY) ajoute une unité de charge à ce qui reste des coups précédents.
- La charge **fuit** avec une constante de temps égale au cinquième de la durée de la voix
  (de 15 ms pour un son très court à 300 ms pour un son long).
- Un coup isolé laisse une charge de 1 : **aucun effet**. Seul l'excédent au-dessus de 1 (« la chaleur »)
  agit, et il entre en douceur : `poids = 1 − e^(−2 × chaleur)`, `pousse = 1 + 0,6 × chaleur`.
- L'étage de sortie est modélisé par un plafond souple : intact jusqu'à 0,9, puis arrondi vers 1,6.
  Le niveau ne baisse donc jamais : il monte, puis s'écrase doucement.

### Mesure (grosse caisse du kit MINI, crête du 4ᵉ coup par rapport à un coup seul)

| Écart entre les coups | Exemple | Crête |
|---|---|---|
| 1 s | coups isolés | ×1,00 (propre) |
| 250 ms | croches à 120 BPM | ×1,00 (propre) |
| 125 ms | doubles croches à 120 BPM | ×1,08 (+0,7 dB, juste plus chaud) |
| 60 ms | MULTIPLY ×2 à 120 BPM | ×1,23 (+1,8 dB) |
| 30 ms | MULTIPLY ×4 à 120 BPM | ×1,57 (+3,9 dB, ça craque) |
| 15 ms | roulement très serré | ×1,63 (+4,2 dB) |

Un son court (charleston, clave) récupère beaucoup plus vite : il ne sature presque que sur des
MULTIPLY ×3 ou ×4.

Vérifié par le test `multiply_hits_saturate` : propre à 1 s, montée continue quand les coups se
rapprochent, et 2 coups dans un pas saturent moins que 4.
