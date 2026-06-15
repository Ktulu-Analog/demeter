; ── Messages personnalisés NSIS — Demeter ──────────────────────────────────

!define MUI_WELCOMEPAGE_TITLE "Bienvenue dans Demeter 2.0"
!define MUI_WELCOMEPAGE_TEXT "Cet assistant va installer Demeter sur votre ordinateur.$\r$\n$\r$\nDemeter est un assistant IA orienté RH, fonctionnant en local et communiquant avec les API Albert et Anthropic.$\r$\n$\r$\nCliquez sur Suivant pour continuer."

!define MUI_LICENSEPAGE_TEXT_TOP "Veuillez lire attentivement le contrat de licence avant d'installer Demeter."
!define MUI_LICENSEPAGE_TEXT_BOTTOM "Si vous acceptez les termes de ce contrat, cochez la case ci-dessous."
!define MUI_LICENSEPAGE_CHECKBOX
!define MUI_LICENSEPAGE_CHECKBOX_TEXT "J'accepte les termes du contrat de licence."

!define MUI_DIRECTORYPAGE_TEXT_TOP "Choisissez le dossier d'installation de Demeter."
!define MUI_DIRECTORYPAGE_TEXT_DESTINATION "Dossier de destination :"

!define MUI_INSTFILESPAGE_FINISHHEADER_TEXT "Installation terminée"
!define MUI_INSTFILESPAGE_FINISHHEADER_SUBTEXT "Demeter a été installé avec succès."
!define MUI_INSTFILESPAGE_ABORTHEADER_TEXT "Installation interrompue"
!define MUI_INSTFILESPAGE_ABORTHEADER_SUBTEXT "L'installation n'a pas abouti."

!define MUI_FINISHPAGE_TITLE "Demeter est prêt !"
!define MUI_FINISHPAGE_TEXT "L'installation de Demeter 2.0 est terminée.$\r$\n$\r$\nCliquez sur Terminer pour fermer cet assistant."
!define MUI_FINISHPAGE_RUN "$INSTDIR\Demeter.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Lancer Demeter maintenant"
!define MUI_FINISHPAGE_LINK "github.com/Ktulu-Analog/demeter"
!define MUI_FINISHPAGE_LINK_LOCATION "https://github.com/Ktulu-Analog/demeter"

!define MUI_UNCONFIRMPAGE_TEXT_TOP "Demeter va être désinstallé de votre ordinateur."
!define MUI_UNCONFIRMPAGE_TEXT_LOCATION "Dossier à désinstaller :"
