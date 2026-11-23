# Ritmo-6 — routage audio (sortie Mix + 6 sorties séparées)

*Version 1.3.1. Valable pour le VST3 et le CLAP ; l'appli autonome n'a que la sortie Mix.*

## Les 7 sorties du plugin

| Sortie | Canaux | Ce qu'elle contient |
|---|---|---|
| **Mix** (principale) | stéréo | les voix **non coupées par MUTE**, avec PAN et PAN RND, **+ les retours d'effets**, le tout × MASTER VOL |
| **Voice A** … **Voice F** | stéréo (le même signal à gauche et à droite) | une seule voix, **sèche** : après son curseur de volume, **sans** PAN, **sans** effets, **sans** MASTER VOL, et **MUTE n'agit pas dessus** |

## La règle simple

> **MUTE allumé = la voix quitte le Mix, mais continue sur sa sortie séparée.**

Pour envoyer une voix sur sa propre piste sans l'entendre en double : branchez la piste sur sa sortie
(« Voice A »…) et allumez le **MUTE** de cette voix sur Ritmo-6. Elle n'est plus dans le Mix, ni ses effets.
Pour couper vraiment la voix partout, coupez aussi sa piste dans votre logiciel.

## Le chemin d'une voix

```
              son de la voix (module A01…F06, TUNE, DRIFT…)
                         │
                 × curseur de volume de la voie
                         │
         ┌───────────────┴──────────────────────────────┐
         │                                              │
         ▼                                              ▼
  sortie « Voice A »                           MUTE allumé ? ── oui ──▶ (rien dans le Mix)
  (mono, sèche ; MUTE et                                 │ non
   MASTER VOL n'agissent pas)                            ▼
                                                  PAN + PAN RND
                                                         │
                                          ┌──────────────┴───────────────┐
                                          ▼                              ▼
                                   somme stéréo des             envois PH · DS · DL · SP
                                     voix du Mix                (après volume et pan)
                                          │                              │
                                          │                  PHASER · DISTORTION · DELAY
                                          │                      · SPRING REVERB
                                          └──────────────┬───────────────┘
                                                         ▼
                                                  × MASTER VOL
                                                         ▼
                                                   sortie « Mix »
```

Ce qui agit **sur les deux** sorties : le son du module, le curseur de volume, **ONE / ALL**
(une rangée hors solo ne joue plus du tout), NUDGE, SWING et les DRIFT.

Ce qui n'agit **que sur le Mix** : **MUTE**, PAN, PAN RND, les envois et les 4 effets, MASTER VOL.

## Exemples dans Ableton Live

**1. Tout dans le Mix (le plus simple)**
Aucune piste pour les sorties séparées. MUTE coupe la voix comme d'habitude.

**2. Une ou deux voix sur leur propre piste, le reste dans le Mix**
- Créez une piste audio : *Audio From* = la piste Ritmo-6, puis *Voice A* ; *Monitor* = **In**.
- Sur Ritmo-6, allumez **MUTE** sur la voix A.
- Résultat : la voix A n'est que sur sa piste (traitez-la à part), les autres voix et les effets restent dans le Mix.

**3. Les 6 voix sur 6 pistes**
- Créez 6 pistes audio qui reçoivent *Voice A* … *Voice F*.
- Allumez les 6 **MUTE** sur Ritmo-6 (ou mettez la piste Ritmo-6 à −∞ : son volume n'agit que sur le Mix).
- Les effets du panneau FX ne sont alors plus entendus : utilisez ceux du logiciel sur chaque piste.

## Vérifier soi-même le routage

- Faites jouer une voix (ou une seule rangée avec **ONE**) et créez une piste qui reçoit sa sortie.
- **MUTE éteint** : vous entendez la voix dans le Mix **et** sur sa piste (le niveau monte).
- **MUTE allumé** : la piste de la voix joue toujours, le Mix ne la contient plus.
- Coupez la piste dans le logiciel : plus rien.

## En chiffres (pour les curieux)

- Volume de la voie : linéaire, 10 V = gain 1 (0 dB).
- PAN : loi à puissance constante (−3 dB par côté au centre) ; les sorties séparées n'ont pas de pan,
  donc une voix centrée est environ 3 dB plus forte sur sa sortie séparée que dans le Mix.
- MASTER VOL : linéaire, 10 V = gain 1, appliqué au Mix seulement ; le Mix est limité à ±4 (+12 dB).
- Avant la version 1.3.1, MUTE coupait aussi la sortie séparée.
