# Recette locale SurvivorsForge v0.5.3

Date : 2 novembre 2026. Worktree : `SurvivorsForge-v053-integration` (HEAD `6a696f1`, plus les correctifs de l'audit et de cette recette). Aucun commit, push, reset, clean ni suppression.

Verdict : **PASS WITH RESERVATIONS**.

L'intégration du périmètre déjà porté (issues, commentaires, pagination, sélecteur de langue, administration des signalements) est utilisable sur une pile locale isolée, après trois corrections reproductibles. Elle n'est pas un produit final : la synchronisation bidirectionnelle GitHub et la traduction intégrale restent des lots distincts, volontairement non commencés.

## Pile utilisée

La pile `survivorsforge-local` était déjà saine sur `127.0.0.1:3080-3081`, avec le volume `survivorsforge-local_postgres` et les images `survivorsforge/forge-api:local` et `survivorsforge/forge-web:local` (2 jours). Elle n'a pas été reconstruite ni arrêtée. Le port hôte `5432` est occupé par une autre base et n'a pas été utilisé. Les conteneurs `sf-v053-postgres-test` et `survivorsforge-v053-sql-test` n'ont pas été modifiés.

Pile dédiée, projet Compose `survivorsforge-v053-recette` :

| Élément | Valeur |
| --- | --- |
| Fichiers | `compose.yaml` + `compose.recette.yaml`, environnement `.env.recette` |
| Images | `survivorsforge/forge-api:v053-recette`, `survivorsforge/forge-web:v053-recette` |
| Site | http://127.0.0.1:3180 |
| API | http://127.0.0.1:3181 |
| Sous-réseau | `10.213.53.0/24` |
| Postgres | conteneur interne, aucun port publié, volume `survivorsforge-v053-recette_postgres` |
| Clé GitHub | PEM jetable `secrets/recette-github-app.pem`, identifiants factices |
| Worker | non démarré, pour ne pas appeler GitHub |

Santé après démarrage : `GET /healthz` API `{"db":"ok","status":"ok"}`, site `{"api":"ok","status":"ok"}`. Les 37 migrations `0001` à `0037` sont enregistrées avec `success = true` dans `_sqlx_migrations`. Les journaux de démarrage indiquent l'écoute sur `:8080`. Le site journalise ensuite des échecs d'export OTLP, parce que Tempo n'est pas lancé : bruit attendu, pas un échec fonctionnel.

Données : `scripts/recette-seed.sql`. Quatre comptes synthétiques (`recette-admin`, `recette-mod`, `recette-author`, `recette-reader`, identifiants GitHub `900001` à `900004`), 25 addons, 25 versions, 2 captures, 36 tickets, commentaires et un signalement. Aucun compte GitHub réel n'a été utilisé. La page de connexion GitHub s'est affichée une fois par la redirection automatique de `/login` ; aucun identifiant n'y a été saisi, et la navigation est revenue aussitôt sur le site local. Les sessions suivantes passent par un code de connexion inséré dans la base jetable, échangé par `/auth/callback`.

La pile de recette est encore démarrée. Elle se retire avec `docker compose --env-file .env.recette -p survivorsforge-v053-recette -f compose.yaml -f compose.recette.yaml down` sans `-v` si le volume doit être conservé. Ne pas lancer cette commande sur le projet `survivorsforge-local`.

## Corrections faites pendant la recette

1. `/static/i18n.js` répondait 404 : le modèle le référençait, mais `app.rs` ne servait pas le fichier. Route, `include_str!` et empreinte de cache ajoutés. Le test `the_language_selector_is_on_every_page` vérifie maintenant que le script répond 200 et contient `ForgeI18n`. Résultat : 1 passé.
2. Le filtre des tickets était un script inline, bloqué par `script-src 'self'`. Il est déplacé dans `static/view.js`. Rejoué dans le navigateur.
3. Le formulaire de commentaire d'un ticket postait vers `/addons/{slug}/issues/{id}/comments`, chemin sans route (404). La route POST a été ajoutée à côté du POST déjà présent sur la fiche. Rejoué : le commentaire « Discussion de recette sur le ticket. » apparaît dans « Diskussion (1) ».

## Résultats

| Fonction | Résultat | Preuve |
| --- | --- | --- |
| Santé API et site, 37 migrations | PASS | Réponses `/healthz` et table `_sqlx_migrations` |
| Accueil public | PASS | Addon de recette listé, navigation et pied de page présents |
| Catalogue, page 2 | PASS | `?page=2` affiche « 21–25 of 25 addons » et les fillers 01 à 05 |
| Recherche addons | PASS | `q=acceptance` ramène 1 addon, « Recette Addon » |
| Fiche addon | PASS | Version 1.0.25, liens Comments et Issues, description de recette |
| Lightbox | PASS | Ouverture, suivant, légende « Second recipe screenshot », compteur 2/2. Capture `docs/recette-captures/lightbox.png` |
| Historique des versions | PASS | Page 1 : 1.0.25 à 1.0.06, résumé « 1–20 of 25 ». Page 2 : 1.0.05 à 1.0.01, « 21–25 of 25 versions » |
| Téléchargement d'un zip réel | NON TESTÉ | Les fichiers pointent vers `example.invalid`. Pas de worker ni de dépôt GitHub |
| Tickets, filtre ouvert | PASS | Après correctif : « In progress » et « Acknowledged » visibles, « Closed » et « Resolved » masqués |
| Tickets, filtre fermé | PASS | Seulement « Closed sample » et « Resolved sample » |
| Tickets, recherche | PASS | « Acknowledged » ne laisse que l'échantillon correspondant |
| Tickets, page suivante | PASS | `?before=7` affiche les 6 tickets restants, sans lien suivant |
| Création de ticket | PASS | « Recette issue from admin #37 », statut initial ouvert |
| Changement de statut, y compris acknowledged | PASS | Statut `acknowledged`, libellé allemand « Bestätigt », option présente dans la liste |
| Commentaire sur un ticket | PASS | Après ajout de la route. « Diskussion (1) » |
| Commentaires publics sans session | PASS | Lecture, pas de formulaire, pas de bouton répondre, lien « Ältere Kommentare » |
| Pagination des commentaires | PASS | `?before=6` affiche les 5 plus anciens, sans lien suivant |
| Commentaire, réponse, citation | PASS | Compte `recette-admin`. Réponse avec `parent` 36, citation avec lien « ↳ #36 » |
| Édition inline | PASS | Texte « Commentaire de recette admin, modifie » et marque d'édition |
| Suppression avec confirmation | PASS | Annulation : message « Diesen Kommentar löschen ? », commentaire conservé. Confirmation : réponse supprimée, citation conservée |
| Sélecteur de langue | PASS | Allemand appliqué (« Mit GitHub anmelden », « Pakete ») et conservé dans `localStorage` (`forge-locale=de`) après navigation |
| Traduction de tout le site | NON TESTÉ | Lot distinct. Beaucoup de chaînes de contenu restent en anglais. « Documentation » et « Statistics » restent les ancres prévues par l'audit |
| Documentation | PASS | Hub `/docs` et ouverture de `/docs/creator-guide` |
| Statistiques publiques | PASS | Sections site, lanceur, addons, plateforme et contrôles affichées |
| Packs | PASS pour la liste vide et le basculement tuiles/liste | Création, zip et téléchargement : NON TESTÉ (aucun binaire GitHub) |
| Administration anonyme | PASS | `/admin` envoie vers la connexion. Aucun compte GitHub saisi |
| Session admin synthétique | PASS | « Abmelden recette-admin », lien Administration |
| File des signalements de commentaires | PASS | Tuile « Kommentarmeldungen », signalement spam sur le commentaire 33, note obligatoire, « Dismiss report » aboutit à « No pending comment reports ». Capture `docs/recette-captures/admin.png` |
| Modérateur | PASS | `/admin` en 200, tuile absente, `/admin/comment-reports` en 403 |
| Nouveau signalement soumis par le formulaire public | NON TESTÉ | La file et sa résolution ont été exercées sur le signalement déjà en base |
| Création d'addon, synchro, vérification, membres | NON TESTÉ | Ces parcours exigent une GitHub App réelle, hors autorisation |
| Synchronisation bidirectionnelle des tickets GitHub | NON TESTÉ | Lot distinct, non commencé. Worker non démarré |
| Grafana et profil monitoring | NON TESTÉ | Profil non lancé |

## Anomalies

Corrigées et rejouées : script i18n absent, filtre de tickets bloqué par la politique de sécurité, commentaire de ticket en 404.

Observées, non traitées comme des défauts bloquants :

- Le libellé de déconnexion allemand se colle au nom (« Abmeldenrecette-admin ») : la traduction n'a pas l'espace final du texte anglais.
- Export OTLP en erreur tant que Tempo n'est pas démarré.
- Les images de la lightbox sont des WebP 1×1 de test ; le visualiseur fonctionne, le contenu graphique n'est pas représentatif.

## Réserves

- Comptes synthétiques seulement. Le parcours OAuth GitHub réel n'a pas été fait.
- Locale choisie dans le navigateur. `negotiate()` côté serveur n'alimente toujours pas le HTML initial.
- Le tri paginé des versions n'ajoute pas `version_key` comme départage.
- La traduction intégrale et la synchro GitHub ne font pas partie de ce lot.
- Le worker, les scans, les packs réels et les actions créateur GitHub n'ont pas été exercés.

## Conclusion

L'intégration v0.5.3 du périmètre communautaire déjà porté est **prête à être finalisée**, avec les réserves ci-dessus. Elle n'est pas prête à être présentée comme incluant la synchronisation GitHub ni la traduction complète du site : ces deux chantiers restent à ouvrir séparément. Aucun changement n'a été commité.
