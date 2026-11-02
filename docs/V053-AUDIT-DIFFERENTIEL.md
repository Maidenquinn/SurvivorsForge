# Audit différentiel — intégration upstream v0.5.3

Date : 2 novembre 2026.

Verdict : **PASS WITH RESERVATIONS**.

La cible `SurvivorsForge-v053-integration` (HEAD détaché `6a696f132db2c139c980cd0577ef09752ed97296`, upstream v0.5.3) conserve les fonctions du fork présentes dans le répertoire de travail de `SurvivorsForge-v050-integration` (branche `integration/upstream-050-features`, `26dcb9a`, ancêtre de v0.5.3). Les écarts qui rendaient le suivi d’issues, les commentaires et le sélecteur de langue inutilisables ont été corrigés dans la cible seulement. Le dépôt original `SurvivorsForge`, le worktree v0.5.0, le stash et aucune base n’ont été modifiés.

## Périmètre comparé

Source de vérité : contenu réel du worktree v0.5.0, y compris fichiers non suivis. Les commits seuls ne suffisent pas : tout le travail du fork est non commité au-dessus de `26dcb9a`.

| Élément | Constat |
| --- | --- |
| v0.5.0 suivi | 13 fichiers, +252 / −6 par rapport à `26dcb9a` |
| v0.5.3 suivi avant correction | mêmes 13 fichiers, même volume, hunks adaptés au contexte v0.5.3 |
| Fichiers ajoutés identiques | routes issues/commentaires, modules web, `i18n.rs`, `i18n.js`, 4 modèles HTML |
| Migrations | 5 fichiers, contenu identique, renumérotés 0032–0036 → 0033–0037 car `0032_verification_requests.sql` est upstream |
| Stash `stash@{0}` (`pre-v052-integration`) | 6 fichiers, +76 lignes : sous-ensemble antérieur du câblage routes/templates. Aucune fonction absente du worktree. Non appliqué, non supprimé |
| `origin/feat/internal-issues` | 21 commits devant la branche locale `feat/internal-issues`. Sert de référence comportementale (CSS, script, sélecteur) quand le worktree a perdu le branchement lors du rebase sur upstream |

Les sauvegardes `*.before-*`, journaux et `target-clean-v052` du worktree v0.5.0 n’ont pas été portés.

## Matrice de couverture

| Fonction | Classe | Fichiers | Écart constaté | Correction dans la cible |
| --- | --- | --- | --- | --- |
| Issues internes (CRUD, statuts, accusé, édition, suppression) | PRESERVEE | `routes/issues.rs`, `issues.rs`, `issue.html`, `issues.html`, migrations 0033–0035 | Identique au worktree v0.5.0 | Aucune sur l’API |
| Filtre « ouvertes » incluant `acknowledged` | PARTIELLE → corrigée | `issues.html` | Le filtre du worktree masquait le statut `acknowledged`, pourtant autorisé par la migration et l’API. La branche `feat/internal-issues` le comptait parmi les ouvertes | Filtre aligné ; option « Acknowledged » ajoutée au formulaire de statut |
| Commentaires, fils, citation, signalement, soft-delete | PRESERVEE | `routes/comments.rs`, `comments.rs`, `comments.html`, migrations 0036–0037 | Logique et modèles identiques | Aucune sur l’API |
| Permissions commentaires | PRESERVEE | `routes/comments.rs` | Auteur ou admin (`Moderate`) peut éditer ; modérateur d’addon peut supprimer sans éditer. Contrôles d’isolation par `addon_id` | Aucune |
| File admin des signalements de commentaires | PARTIELLE → corrigée | `admin.rs`, `admin/index.html`, `reports.rs` | La route existait, mais l’accueil admin ne la montrait pas. L’API exige `manage_roles` (admin) alors que la page web acceptait tout le staff | Tuile réservée aux admins ; un modérateur reçoit 403 avant l’appel API |
| Note obligatoire à la résolution | PRESERVEE | `reports.rs`, `comment_reports.html` | `note_required` + champ `required` | Aucune |
| Pagination des versions | PARTIELLE → adaptée | `versions.rs`, `public.rs`, `versions.html` | Prev/next seulement, alors que v0.5.3 fournit le composant `Pager` numéroté pour `/addons` | La page versions utilise `Pager` (`total`, `page`, `per_page`) |
| Pagination addons, packs, admin, audit | REMPLACEE PAR UPSTREAM ou PRESERVEE | `addons.html`, `packs/list.html`, `admin/addons.html`, `admin/audit.html` | Numéros de page upstream (v0.5.3) et prev/next packs/admin déjà présents. Le fork n’avait pas une seconde pagination à réécrire | Non touché |
| Pagination curseur issues et commentaires (31 lignes, page 30, pas de page fantôme) | PRESERVEE | `issues.rs`, `comments.rs` | `LIMIT 31` + `next_cursor` testé unitairement dans le module web | Aucune |
| i18n FR/EN et 12 autres langues du jeu | PARTIELLE → partiellement corrigée | `i18n.js`, `i18n.rs`, `base.html`, `scripts/` | Catalogue et `data-i18n` des pages communauté copiés, mais `i18n.js` n’était inclus nulle part et `#forge-locale` n’existait pas. `i18n.rs` (`negotiate`) n’est branché ni dans le worktree ni dans la cible. Scripts de contrôle absents de la cible | Script + sélecteur 14 langues dans l’en-tête, sans retirer `aria-current`, le lien `/docs` ni `/stats`. Scripts copiés. Clé manquante `older_issues` ajoutée dans les 14 catalogues |
| Galerie et lightbox | REMPLACEE PAR UPSTREAM | `lightbox.html`, `view.js`, `addon.html` | Upstream v0.5.3 a déjà le dialogue, le zoom et la navigation. La branche fork avait un second lightbox. Le recopier doublerait les écouteurs | Conservé tel quel |
| Onglets dynamiques sur la fiche addon | REMPLACEE PAR UPSTREAM / choix du worktree | `addon.html` | La branche embarquait commentaires et issues dans des onglets. Le worktree v0.5.0, puis la cible, exposent deux liens « Comments » et « Issues » | Comportement du worktree conservé |
| Chargement progressif des commentaires, réponse, citation, édition inline, confirmation de suppression | PARTIELLE → corrigée | `view.js`, `app.css` | Les attributs `data-dynamic-comments`, `data-edit-toggle`, `data-confirm-delete` étaient dans les modèles, sans JS ni CSS (perdus au rebase). Les formulaires POST restaient le repli | JS et CSS communautaires ajoutés par-dessus `view.js` / `app.css` v0.5.3. `data-confirm` upstream inchangé |
| Modération, vérification, notes, actions groupées | REMPLACEE PAR UPSTREAM | admin review, `verification_requests` | Présents dans v0.5.3 (`0032_verification_requests.sql`, files, notes, bulk). Le port ne les a pas retirés | Non touché |
| Pages publiques, créateurs, recherche, stats, navigation rouge | REMPLACEE PAR UPSTREAM | `public.rs`, `base.html`, stats | v0.5.3 : pager toujours visible, section courante, documents sous `/docs` | Conservés. Le sélecteur est un `<span>` interne pour ne pas casser `aria-current="page">` |
| Synchronisation bidirectionnelle GitHub des issues | ABSENTE DU FORK | — | Aucune occurrence dans le worktree, le stash, ni `origin/feat/internal-issues`. La synchro GitHub existante est celle des addons (upstream) | Non inventée |
| Migrations | PRESERVEE | `0033`–`0037` | 37 fichiers, préfixes uniques `0001`–`0037`, contenu des 5 migrations fork identique au v0.5.0 | Aucune réécriture SQL |
| Tests et documentation fork | PARTIELLE | `tests/public.rs`, `tests/creator.rs`, `docs/` | Les docs `comments-i18n-implementation.md` et `internal-issues-local-test.md` sont sur la branche, pas dans le worktree v0.5.0. Les tests web du port couvraient la page versions, pas le sélecteur ni la file commentaires | Tests ciblés ajoutés. Les docs de branche ne sont pas recopiées : elles décrivent un contrat antérieur aux pages séparées |

## Corrections

Uniquement sous `SurvivorsForge-v053-integration`.

1. **En-tête** (`templates/base.html`) : chargement de `/static/i18n.js` avant `view.js`, libellés de navigation via `<span data-i18n>`, sélecteur `#forge-locale` (14 langues, noms natifs). Le rôle staff upstream (`u.staff`) est conservé : la branche utilisait `u.admin`, ce qui aurait caché l’admin aux modérateurs. Le pied de page garde Documentation (`/docs`) et Statistics (`/stats`).
2. **Styles** (`static/app.css`) : issues, discussion, fils, signalement, sélecteur. Le lightbox upstream n’est pas remplacé.
3. **Scripts** (`static/view.js`) : confirmation `data-confirm-delete` dans l’écouteur de capture déjà utilisé pour `data-confirm` ; édition inline ; mise à jour des commentaires sans rechargement, avec repli formulaire ; imbrication des réponses ; réponse et citation. Les onglets addon et le second lightbox de la branche ne sont pas réintroduits.
4. **Versions** : `Pager` numéroté à la place du seul couple précédent/suivant. L’endpoint `/v1/addons/{slug}/versions/page` (limite 1–100, défaut 20) est inchangé.
5. **Issues** : filtre et choix de statut `acknowledged`.
6. **Admin** : tuile « Comment reports » si `Permission::ManageRoles`. Les handlers web refusent un modérateur avant l’API.
7. **Catalogue** : clé `older_issues` dans les 14 langues. `scripts/check-i18n.mjs` échouait sinon sur `issues.html`, déjà dans le worktree v0.5.0.
8. **Scripts** copiés depuis le worktree v0.5.0 : `scripts/check-i18n.mjs`, `scripts/audit-i18n.mjs`.

## Tests exécutés

Hors base. `cargo` n’est pas sur le PATH Windows de cette session ; les tests Rust ont tourné dans l’image Docker `rust:1-bookworm`, `--locked`, répertoire monté en lecture-écriture (artefacts dans `target/`, ignoré par git).

| Contrôle | Résultat |
| --- | --- |
| `node --check` sur `view.js` | OK |
| `node scripts/check-i18n.mjs` | OK : 14 langues, 478 clés, 63 modèles, références valides |
| `cargo test -p forge-web --test admin --locked` filtres `comment_reports_are_an_admin_page`, `verification_requests_are_listed_and_decided` | OK |
| `cargo test -p forge-web --test public --locked` filtres `version_pages_use_the_numbered_pager`, `version_history` | OK |
| `cargo test -p forge-web --test skeleton --locked` filtres `the_language_selector_is_on_every_page`, `the_nav_link_of_the_current_section_is_marked` | OK |
| `cargo test -p forge-web --test creator --locked` filtre `the_edit_toolbar_stays_on_every_edit_page` | OK |
| `node scripts/audit-i18n.mjs` | 773 chaînes encore sans marque `data-i18n` (voir réserves). Le script n’est pas en mode `--strict` |
| Intégrité migrations | 37 fichiers, aucun préfixe dupliqué. Pas de ré-application : l’audit ne touche aucune base. Les 37 migrations déjà appliquées sur la base jetable restent la référence d’application |

Non relancé, faute de changement dans ces crates : suite API complète, `forge-scan`, launcher, `cargo check --workspace`.

## Recette navigateur locale

Non exécutée dans cette session : pas de base jetable démarrée, et le binaire web n’a pas été servi. Les tests ci-dessus rendent le HTML (sélecteur, pager, tuile admin, 403 modérateur) sans navigateur.

À faire sur la base locale Compose, jamais sur la base de production. Depuis `SurvivorsForge-v053-integration`, avec `.env.local` de test (voir `docs/local-setup.md`) :

```powershell
function dcl { docker compose --env-file .env.local -f compose.yaml -f compose.local.yaml @args }
dcl up -d --build
```

Puis, sur http://localhost:3080 :

1. Accueil : le `<select id="forge-locale">` est visible. Choisir Français traduit la navigation (« Mes AddOns », « Se connecter avec GitHub ») et le pied de page connu. Recharger : la langue reste (`localStorage` `forge-locale`). Les liens `/docs` et `/stats` restent. L’entrée de menu de la section courante garde `aria-current="page"`.
2. Fiche addon : liens Comments et Issues. Les captures ouvrent la lightbox upstream (précédent, suivant, zoom).
3. `/addons/{slug}/versions` : résumé du type `1–20 of N versions`, numéros de page, page courante. Une seule page affiche quand même le pager, comme `/addons`.
4. `/addons/{slug}/issues` : créer un ticket, filtrer Open / Closed / All (un statut `acknowledged` reste dans Open), ouvrir le ticket, répondre, changer le statut vers Acknowledged. Sans JavaScript, les formulaires POST fonctionnent encore.
5. `/addons/{slug}/comments` : poster, répondre, citer, éditer sur place, supprimer (confirmation), signaler. La pagination `?before=` met à jour la liste sans quitter la page. Un second clic sur Annuler retire le contexte de réponse.
6. Compte admin : tuile Comment reports, note obligatoire, résoudre ou retirer. Compte modérateur : pas de tuile, `/admin/comment-reports` en 403. Les demandes de vérification upstream restent sur l’accueil admin.
7. `/addons` : recherche, tri, pager numéroté upstream toujours visible.

## Réserves

1. **Navigateur non joué.** La recette ci-dessus reste à exécuter sur une base locale vide ou jetable.
2. **i18n de surface incomplet.** Le sélecteur et les pages communauté fonctionnent. `scripts/audit-i18n.mjs` signale encore 773 chaînes anglaises sans marque, surtout créateur, admin, packs et accueil. Le worktree v0.5.0 était dans le même état hors pages communauté. Les traductions ne sont pas inventées pour tout le site v0.5.3.
3. **`i18n.rs` n’alimente pas le rendu serveur.** La langue est appliquée au chargement par `i18n.js` (préférence explicite, sinon langue du navigateur, sinon anglais). Il n’y a pas de cookie validé ni de `Accept-Language` côté Askama. Le HTML initial est anglais, puis remplacé. C’est le mécanisme réellement livré par le catalogue client.
4. **Quelques valeurs hors FR/EN du catalogue étaient déjà en anglais** pour les toutes dernières clés de signalement (`comment_reported_by`, etc.). `check-i18n.mjs` exige seulement une chaîne non vide. Non modifié, pour ne pas substituer une traduction non revue.
5. **Pas de synchronisation bidirectionnelle des issues avec GitHub.** Absente du fork. Les issues sont internes. La synchro GitHub des dépôts d’addons reste celle d’upstream.
6. **Tri des versions paginées.** `paged` ordonne par `published_at DESC, id DESC`. La liste legacy re-trie les tags de même date par numéro de version. Comportement identique au worktree, pas une régression v0.5.3.
7. **Création d’issue sur un addon bloqué.** Le handler renvoie l’erreur « Only addon maintainers can change issue status » au lieu d’une erreur de blocage. Présent tel quel dans le worktree. Les lectures utilisent `not_blocked`.
8. **Docs de la branche** `docs/comments-i18n-implementation.md` et `docs/internal-issues-local-test.md` ne sont pas dans le worktree v0.5.0. Elles ne sont pas copiées : elles décrivent les onglets embarqués et un rendu serveur de locale qui n’a pas été le code intégré.

## Fichiers de la cible touchés par les corrections

- `crates/forge-web/templates/base.html`
- `crates/forge-web/templates/admin/index.html`
- `crates/forge-web/templates/versions.html`
- `crates/forge-web/templates/issues.html`
- `crates/forge-web/templates/issue.html`
- `crates/forge-web/src/admin.rs`
- `crates/forge-web/src/public.rs`
- `crates/forge-web/src/templates.rs`
- `crates/forge-web/static/app.css`
- `crates/forge-web/static/view.js`
- `crates/forge-web/static/i18n.js`
- `crates/forge-web/tests/admin.rs`
- `crates/forge-web/tests/public.rs`
- `crates/forge-web/tests/skeleton.rs`
- `scripts/audit-i18n.mjs`
- `scripts/check-i18n.mjs`
- `docs/V053-AUDIT-DIFFERENTIEL.md`
