// Client-side UI labels; never translate user-generated content or addon descriptions.
(function () {
  "use strict";
  var translations = {
  "en": {
    "addons": "Addons",
    "packs": "Packs",
    "my_addons": "My addons",
    "admin": "Admin",
    "login": "Log in with GitHub",
    "language": "Language",
    "comments": "Community comments",
    "your_comment": "Your comment",
    "post": "Post comment",
    "edit": "Edit",
    "delete": "Delete",
    "cancel": "Cancel",
    "save": "Save changes",
    "overview": "Overview",
    "issues": "Issues",
    "gallery": "Gallery",
    "versions": "Versions",
    "comments_tab": "Comments"
  },
  "fr": {
    "addons": "Extensions",
    "packs": "Packs",
    "my_addons": "Mes extensions",
    "admin": "Administration",
    "login": "Se connecter avec GitHub",
    "language": "Langue",
    "comments": "Commentaires de la communauté",
    "your_comment": "Votre commentaire",
    "post": "Publier",
    "edit": "Modifier",
    "delete": "Supprimer",
    "cancel": "Annuler",
    "save": "Enregistrer",
    "overview": "Présentation",
    "issues": "Signalements",
    "gallery": "Galerie",
    "versions": "Versions",
    "comments_tab": "Commentaires"
  },
  "pl": {
    "addons": "Dodatki",
    "packs": "Pakiety",
    "my_addons": "Moje dodatki",
    "admin": "Administracja",
    "login": "Zaloguj przez GitHub",
    "language": "Język",
    "comments": "Komentarze społeczności",
    "your_comment": "Twój komentarz",
    "post": "Opublikuj",
    "edit": "Edytuj",
    "delete": "Usuń",
    "cancel": "Anuluj",
    "save": "Zapisz zmiany",
    "overview": "Przegląd",
    "issues": "Zgłoszenia",
    "gallery": "Galeria",
    "versions": "Wersje",
    "comments_tab": "Komentarze"
  },
  "cs": {
    "addons": "Doplňky",
    "packs": "Balíčky",
    "my_addons": "Moje doplňky",
    "admin": "Správa",
    "login": "Přihlásit se přes GitHub",
    "language": "Jazyk",
    "comments": "Komentáře komunity",
    "your_comment": "Váš komentář",
    "post": "Odeslat komentář",
    "edit": "Upravit",
    "delete": "Smazat",
    "cancel": "Zrušit",
    "save": "Uložit změny",
    "overview": "Přehled",
    "issues": "Hlášení",
    "gallery": "Galerie",
    "versions": "Verze",
    "comments_tab": "Komentáře"
  },
  "de": {
    "addons": "Addons",
    "packs": "Pakete",
    "my_addons": "Meine Addons",
    "admin": "Administration",
    "login": "Mit GitHub anmelden",
    "language": "Sprache",
    "comments": "Community-Kommentare",
    "your_comment": "Dein Kommentar",
    "post": "Kommentar senden",
    "edit": "Bearbeiten",
    "delete": "Löschen",
    "cancel": "Abbrechen",
    "save": "Änderungen speichern",
    "overview": "Übersicht",
    "issues": "Probleme",
    "gallery": "Galerie",
    "versions": "Versionen",
    "comments_tab": "Kommentare"
  },
  "pt-BR": {
    "addons": "Complementos",
    "packs": "Pacotes",
    "my_addons": "Meus complementos",
    "admin": "Administração",
    "login": "Entrar com GitHub",
    "language": "Idioma",
    "comments": "Comentários da comunidade",
    "your_comment": "Seu comentário",
    "post": "Publicar comentário",
    "edit": "Editar",
    "delete": "Excluir",
    "cancel": "Cancelar",
    "save": "Salvar alterações",
    "overview": "Visão geral",
    "issues": "Problemas",
    "gallery": "Galeria",
    "versions": "Versões",
    "comments_tab": "Comentários"
  },
  "es": {
    "addons": "Complementos",
    "packs": "Paquetes",
    "my_addons": "Mis complementos",
    "admin": "Administración",
    "login": "Iniciar sesión con GitHub",
    "language": "Idioma",
    "comments": "Comentarios de la comunidad",
    "your_comment": "Tu comentario",
    "post": "Publicar comentario",
    "edit": "Editar",
    "delete": "Eliminar",
    "cancel": "Cancelar",
    "save": "Guardar cambios",
    "overview": "Resumen",
    "issues": "Incidencias",
    "gallery": "Galería",
    "versions": "Versiones",
    "comments_tab": "Comentarios"
  },
  "ro": {
    "addons": "Suplimente",
    "packs": "Pachete",
    "my_addons": "Suplimentele mele",
    "admin": "Administrare",
    "login": "Conectare cu GitHub",
    "language": "Limbă",
    "comments": "Comentariile comunității",
    "your_comment": "Comentariul tău",
    "post": "Publică comentariul",
    "edit": "Editează",
    "delete": "Șterge",
    "cancel": "Anulează",
    "save": "Salvează modificările",
    "overview": "Prezentare",
    "issues": "Probleme",
    "gallery": "Galerie",
    "versions": "Versiuni",
    "comments_tab": "Comentarii"
  },
  "tr": {
    "addons": "Eklentiler",
    "packs": "Paketler",
    "my_addons": "Eklentilerim",
    "admin": "Yönetim",
    "login": "GitHub ile giriş yap",
    "language": "Dil",
    "comments": "Topluluk yorumları",
    "your_comment": "Yorumunuz",
    "post": "Yorum gönder",
    "edit": "Düzenle",
    "delete": "Sil",
    "cancel": "İptal",
    "save": "Değişiklikleri kaydet",
    "overview": "Genel bakış",
    "issues": "Sorunlar",
    "gallery": "Galeri",
    "versions": "Sürümler",
    "comments_tab": "Yorumlar"
  },
  "ru": {
    "addons": "Аддоны",
    "packs": "Наборы",
    "my_addons": "Мои аддоны",
    "admin": "Управление",
    "login": "Войти через GitHub",
    "language": "Язык",
    "comments": "Комментарии сообщества",
    "your_comment": "Ваш комментарий",
    "post": "Отправить комментарий",
    "edit": "Изменить",
    "delete": "Удалить",
    "cancel": "Отмена",
    "save": "Сохранить изменения",
    "overview": "Обзор",
    "issues": "Проблемы",
    "gallery": "Галерея",
    "versions": "Версии",
    "comments_tab": "Комментарии"
  },
  "uk": {
    "addons": "Аддони",
    "packs": "Набори",
    "my_addons": "Мої аддони",
    "admin": "Керування",
    "login": "Увійти через GitHub",
    "language": "Мова",
    "comments": "Коментарі спільноти",
    "your_comment": "Ваш коментар",
    "post": "Опублікувати",
    "edit": "Редагувати",
    "delete": "Видалити",
    "cancel": "Скасувати",
    "save": "Зберегти зміни",
    "overview": "Огляд",
    "issues": "Проблеми",
    "gallery": "Галерея",
    "versions": "Версії",
    "comments_tab": "Коментарі"
  },
  "zh-Hans": {
    "addons": "插件",
    "packs": "整合包",
    "my_addons": "我的插件",
    "admin": "管理",
    "login": "使用 GitHub 登录",
    "language": "语言",
    "comments": "社区评论",
    "your_comment": "你的评论",
    "post": "发表评论",
    "edit": "编辑",
    "delete": "删除",
    "cancel": "取消",
    "save": "保存更改",
    "overview": "概览",
    "issues": "问题",
    "gallery": "图库",
    "versions": "版本",
    "comments_tab": "评论"
  },
  "ja": {
    "addons": "アドオン",
    "packs": "パック",
    "my_addons": "マイアドオン",
    "admin": "管理",
    "login": "GitHubでログイン",
    "language": "言語",
    "comments": "コミュニティコメント",
    "your_comment": "コメントを入力",
    "post": "コメントを投稿",
    "edit": "編集",
    "delete": "削除",
    "cancel": "キャンセル",
    "save": "変更を保存",
    "overview": "概要",
    "issues": "問題",
    "gallery": "ギャラリー",
    "versions": "バージョン",
    "comments_tab": "コメント"
  },
  "ko": {
    "addons": "애드온",
    "packs": "팩",
    "my_addons": "내 애드온",
    "admin": "관리",
    "login": "GitHub으로 로그인",
    "language": "언어",
    "comments": "커뮤니티 댓글",
    "your_comment": "내 댓글",
    "post": "댓글 게시",
    "edit": "수정",
    "delete": "삭제",
    "cancel": "취소",
    "save": "변경 사항 저장",
    "overview": "개요",
    "issues": "문제",
    "gallery": "갤러리",
    "versions": "버전",
    "comments_tab": "댓글"
  }
};
  var supported = Object.keys(translations);
  function normalize(value) {
    if (!value) return null;
    var code = value.replace(/_/g, "-").toLowerCase();
    if (code.indexOf("pt") === 0) return "pt-BR";
    if (code.indexOf("zh") === 0) return "zh-Hans";
    return supported.find(function (k) { return k.toLowerCase() === code || code.indexOf(k.toLowerCase() + "-") === 0; }) || null;
  }
  function getSaved() { try { return localStorage.getItem("forge-locale"); } catch (_) { return null; } }
  var locale = normalize(getSaved()) || normalize(navigator.language) || "en";
  function apply() {
    document.documentElement.lang = locale;
    document.querySelectorAll("[data-i18n]").forEach(function (el) {
      var key = el.getAttribute("data-i18n");
      var text = translations[locale][key] || translations.en[key];
      if (text) el.textContent = text;
    });
    var select = document.getElementById("forge-locale");
    if (select) select.value = locale;
  }
  document.addEventListener("DOMContentLoaded", function () {
    apply();
    var select = document.getElementById("forge-locale");
    if (select) select.addEventListener("change", function () {
      locale = normalize(select.value) || "en";
      try { localStorage.setItem("forge-locale", locale); } catch (_) {}
      apply();
    });
  });
  document.addEventListener("htmx:afterSwap", apply);
})();
