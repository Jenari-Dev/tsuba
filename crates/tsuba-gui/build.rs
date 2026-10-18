use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    let builder = CxxQtBuilder::new_qml_module(
        QmlModule::new("io.github.jenaridev.tsuba")
            .qml_file(QmlFile::from("qml/Theme.qml").singleton(true))
            .qml_file(QmlFile::from("qml/KeyNames.qml").singleton(true))
            .qml_file(QmlFile::from("qml/DeviceKinds.qml").singleton(true))
            .qml_files([
                "qml/Main.qml",
                "qml/HomePage.qml",
                "qml/MacrosPage.qml",
                "qml/LinkedGamesPage.qml",
                "qml/GameLightingPage.qml",
                "qml/GameScanDialog.qml",
                "qml/SynapseImportDialog.qml",
                "qml/MouseLeds.qml",
                "qml/LightingOwnerChip.qml",
                "qml/AreaPicker.qml",
                "qml/AppGlyph.qml",
                "qml/SynSlider.qml",
                "qml/SynCard.qml",
                "qml/HelpButton.qml",
                "qml/SynToggle.qml",
                "qml/SynPillTabs.qml",
                "qml/SynDropdown.qml",
                "qml/SynSquareButtons.qml",
                "qml/SynCheckBox.qml",
                "qml/DevicePage.qml",
                "qml/CustomizeTab.qml",
                "qml/PerformanceTab.qml",
                "qml/LightingTab.qml",
                "qml/LightingPage.qml",
                "qml/LedEditor.qml",
                "qml/LedKeyboard.qml",
                "qml/ZoomCanvas.qml",
                "qml/StudioPage.qml",
                "qml/KeyboardView.qml",
                "qml/MouseView.qml",
                "qml/KeyboardImageView.qml",
                "qml/MouseImageView.qml",
                "qml/AssignmentPanel.qml",
                "qml/ModeTabs.qml",
                "qml/SynButton.qml",
                "qml/SectionLabel.qml",
                "qml/EffectIcon.qml",
                "qml/ColourPanel.qml",
                "qml/PictureAlign.qml",
                "qml/PictureControls.qml",
                "qml/SupportFacts.qml",
                "qml/ApproxNote.qml",
                "qml/UntestedBanner.qml",
                "qml/DeviceInfoPage.qml",
                "qml/SettingsPage.qml",
                "qml/AboutPage.qml",
                "qml/FirstRunPanel.qml",
                "qml/DeviceStateBanner.qml",
                "qml/SetupGuide.qml",
                "qml/SupportedDevicesPage.qml",
                "qml/DebounceCard.qml",
                "qml/SmartTrackingCard.qml",
                "qml/DeviceResetCard.qml",
                "qml/WirelessDimCard.qml",
                "qml/DeviceLedsCard.qml",
                "qml/BatteryLightingCard.qml",
            ]),
    )
    .files(["src/backend.rs", "src/app_backend.rs", "src/i18n.rs", "src/device_backend.rs"])
    .include_dir("src")
    .cpp_file("src/i18n.cpp");
    // SAFETY: only adds an include path for the KF6 I18n headers.
    let builder = unsafe { builder.cc_builder(|cc| { cc.include("/usr/include/KF6/KI18n"); cc.include("/usr/include/qt6/QtQmlIntegration"); }) };
    builder.build();
    println!("cargo:rustc-link-lib=KF6I18nQml");
    println!("cargo:rustc-link-lib=KF6I18n");
    println!("cargo:rerun-if-changed=src/i18n.cpp");
    println!("cargo:rerun-if-changed=src/i18n.h");
}
