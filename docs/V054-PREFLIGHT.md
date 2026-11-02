# Pré-audit upstream v0.5.4

Date : 2 novembre 2026. Worktree : `SurvivorsForge-v053-integration`. Aucun merge, rebase, cherry-pick, reset, clean, suppression, push ni changement de base Docker. La pile `survivorsforge-v053-recette` est restée démarrée. Les autres worktrees n'ont pas été touchés.

Verdict : **intégration simple**.

## Sauvegarde v0.5.3

Branche locale `integration/upstream-053-validated`, créée depuis le HEAD détaché `6a696f132db2c139c980cd0577ef09752ed97296`. La branche n'existait pas. Elle n'a pas de suivi distant.

Commit de sauvegarde : `2fd6fa5038097e39ad7d3f2ce52d93cdf06fab09` (`feat(platform): preserve community features validated on v0.5.3`). 38 fichiers, +8933 / −16. Contenu : routes et migrations issues/commentaires `0033`–`0037`, pagination des versions, sélecteur de langue, administration des signalements, tests ciblés, `scripts/check-i18n.mjs`, `scripts/audit-i18n.mjs`, `docs/V053-AUDIT-DIFFERENTIEL.md`, `docs/V053-RECETTE-LOCALE.md`.

Hors commit, encore non suivis : `compose.recette.yaml`, `scripts/recette-seed.sql`, `docs/recette-captures/`. Ignorés et non ajoutés : `.env.recette`, `secrets/recette-github-app.pem`, `target/`. Ce document n'est pas dans le commit de sauvegarde.

## Identité exacte

`git fetch upstream --tags` a seulement ajouté les tags locaux. Aucun checkout.

| Repère | Commit | Date | Sujet |
| --- | --- | --- | --- |
| Tag `v0.5.3` | `8b1357f9715980ae386bd27ae40607d03bb660d1` | 2026-11-01 19:06 UTC | `chore(release): bump version to 0.5.3` |
| Base de notre branche | `6a696f132db2c139c980cd0577ef09752ed97296` | après le tag | `fix(web): name the launcher version in the active-launcher stat` |
| Tag `v0.5.4` | `75a172b7909b8f2828e0f522d6bafcbc83e1911f` | 2026-11-02 08:12 UTC | `chore(release): bump version to 0.5.4` |

Le tag `v0.5.3` est l'ancêtre de notre base, pas notre HEAD. Deux commits upstream sont déjà dans la branche : `4dab105` (documentation de l'API addon au 1er novembre 2026) et `6a696f1` (nom de version du launcher). Le merge-base entre `HEAD` et `v0.5.4` est `6a696f1`.

## Delta réel `6a696f1..v0.5.4`

Deux commits, 6 fichiers, +125 / −4 :

| Commit | Fichiers |
| --- | --- |
| `cac878a` `feat(platform): chart active launchers and downloads per hour in grafana` | migration, `launcher_stats.rs`, test, tableau Grafana |
| `75a172b` `chore(release): bump version to 0.5.4` | `Cargo.toml` `0.5.3` → `0.5.4`, `Cargo.lock` |

Aucune route API, aucun modèle web, aucun template, aucun CSS ni JS dans ce delta. `docs/addon-api.md` et les retouches de tests `public.rs` / `skeleton.rs` vues entre les tags `v0.5.3` et `v0.5.4` sont déjà dans `4dab105` et `6a696f1`.

## Conflits avec nos fonctionnalités

Git n'a aucun fichier en commun entre notre commit et ces deux commits. Issues, commentaires, pagination, `i18n.js`, `view.js`, `app.css` et l'administration des signalements ne sont pas touchés.

Le seul point de portage est le numéro de migration. Upstream ajoute `0033_launcher_counts.sql` (table `launcher_counts`, horodatage, installations actives sur 30 jours, téléchargements). Nos fichiers occupent déjà `0033` à `0037`. Deux fichiers préfixés `0033` feraient échouer sqlx au démarrage, même si Git les accepte côte à côte. Au moment du portage, renuméroter les nôtres en `0034`–`0038` et laisser `0033_launcher_counts.sql` à upstream. Le contenu des tables ne se chevauche pas.

La base de la pile de recette a déjà enregistré nos `0033`–`0037`. Elle ne doit pas recevoir la migration upstream `0033` telle quelle. Le portage se vérifie sur une base neuve.

## Effort et tests à rejouer

Effort court : récupérer les deux commits, renuméroter cinq migrations, compiler. Pas de réécriture fonctionnelle.

À rejouer ensuite :

1. Application des migrations sur une base vide : 38 versions uniques, `0001` à `0038`, `0033` = `launcher_counts`.
2. `cargo test -p forge-api --test launcher_stats` (comportement nouveau : courbe horaire, y compris si GitHub ne répond pas).
3. Tests web déjà verts : sélecteur de langue, pagineur des versions, page admin des signalements, barre créateur (`/versions/page`).
4. Fumée navigateur sur une pile neuve : locale, filtre de tickets, envoi d'un commentaire, tuile des signalements. La pile actuelle reste la référence v0.5.3 et ne sert pas de cible v0.5.4.

## Arrêt

Le pré-audit s'arrête ici. Le portage v0.5.4 n'est pas commencé. Il attend un feu vert explicite.
