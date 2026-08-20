//! C9 — app scaffold: kotlin/swift/flutter + multi-platform (KMP shared).

use std::path::Path;

use anyhow::Result;

use super::{slugify, write_file};

pub struct AppProcessor;

impl AppProcessor {
    pub fn files(target: &Path, name: &str, framework: &str) -> Result<()> {
        match framework {
            "kotlin" => {
                write_file(
                    &target.join("settings.gradle.kts"),
                    &format!("rootProject.name = \"{}\"\n", name),
                )?;
                write_file(
                    &target
                        .join("app")
                        .join("src")
                        .join("main")
                        .join("kotlin")
                        .join("Main.kt"),
                    "fun main() {\n    println(\"MegaGate Kotlin app scaffold\")\n}\n",
                )?;
            }
            "swift" => {
                write_file(
                    &target.join("Package.swift"),
                    &format!(
                        "// swift-tools-version: 5.9\nimport PackageDescription\n\nlet package = Package(\n    name: \"{}\",\n    targets: [.executableTarget(name: \"{}\")]\n)\n",
                        name, name
                    ),
                )?;
                write_file(
                    &target.join("Sources").join(name).join("main.swift"),
                    "print(\"MegaGate Swift app scaffold\")\n",
                )?;
            }
            _ => {
                write_file(
                    &target.join("pubspec.yaml"),
                    &format!(
                        "name: {}\ndescription: MegaGate Flutter app\nversion: 0.1.0\n",
                        slugify(name)
                    ),
                )?;
                write_file(
                    &target.join("lib").join("main.dart"),
                    "void main() {\n  print('MegaGate Flutter app scaffold');\n}\n",
                )?;
            }
        }

        Ok(())
    }

    pub fn files_multi(target: &Path, name: &str) -> Result<()> {
        let slug = slugify(name);
        let pkg = slug.replace('-', "_");

        // shared/ — Kotlin KMP commonMain + androidMain (nơi chứa logic dùng chung)
        write_file(
            &target.join("shared").join("build.gradle.kts"),
            &format!(
                "plugins {{\n    kotlin(\"multiplatform\") version \"2.0.0\"\n}}\n\nkotlin {{\n    androidTarget()\n    iosX64()\n    iosArm64()\n    iosSimulatorArm64()\n\n    sourceSets {{\n        commonMain.dependencies {{\n            implementation(\"org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0\")\n        }}\n    }}\n\n    listOf(\n        iosX64(),\n        iosArm64(),\n        iosSimulatorArm64()\n    ).forEach {{ iosTarget ->\n        iosTarget.binaries.framework {{\n            baseName = \"{name}\"\n            isStatic = true\n        }}\n    }}\n}}\n\nandroid {{\n    namespace = \"{pkg}.shared\"\n    compileSdk = 34\n}}\n"
            ),
        )?;
        write_file(
            &target.join("shared").join("settings.gradle.kts"),
            &format!("rootProject.name = \"{}-shared\"\n", slug),
        )?;
        write_file(
            &target
                .join("shared")
                .join("src")
                .join("commonMain")
                .join("kotlin")
                .join(pkg.as_str())
                .join("Shared.kt"),
            "package {pkg}\n\nobject Shared {\n    fun hello(): String = \"hello from MegaGate shared (KMP)\"\n}\n",
        )?;

        // android/ — kotlin entry dùng shared (gradle include + app module phụ thuộc shared)
        write_file(
            &target.join("android").join("settings.gradle.kts"),
            &format!(
                "rootProject.name = \"{}-android\"\n\ninclude(\":app\", \":shared\")\n\nproject(\":shared\").projectDir = file(\"../shared\")\n",
                slug
            ),
        )?;
        write_file(
            &target.join("android").join("build.gradle.kts"),
            "plugins {\n    id(\"com.android.application\") version \"8.5.0\" apply false\n    kotlin(\"android\") version \"2.0.0\" apply false\n}\n",
        )?;
        write_file(
            &target.join("android").join("app").join("build.gradle.kts"),
            &format!(
                "plugins {{\n    id(\"com.android.application\")\n    kotlin(\"android\")\n}}\n\nandroid {{\n    namespace = \"{pkg}.android\"\n    compileSdk = 34\n    defaultConfig {{\n        applicationId = \"{pkg}.android\"\n        minSdk = 24\n    }}\n}}\n\ndependencies {{\n    implementation(project(\":shared\"))\n}}\n"
            ),
        )?;
        write_file(
            &target.join("android").join("app").join("src").join("main").join("kotlin").join("Main.kt"),
            "package {pkg}.android\n\nimport {pkg}.Shared\n\nfun main() {\n    println(Shared.hello())\n}\n",
        )?;

        // ios/ — swift + objc entry, cùng SPM target
        write_file(
            &target.join("ios").join("Package.swift"),
            &format!(
                "// swift-tools-version: 5.9\nimport PackageDescription\n\nlet package = Package(\n    name: \"{}\",\n    targets: [.executableTarget(name: \"{}\")]\n)\n",
                slug, slug
            ),
        )?;
        write_file(
            &target
                .join("ios")
                .join("Sources")
                .join(slug.as_str())
                .join("main.swift"),
            "print(\"MegaGate iOS app scaffold (swift)\")\n",
        )?;
        // ObjC bridge nằm ngoài SPM Sources (Xcode native project dùng) — SPM không chấp mixed language.
        write_file(
            &target.join("ios").join("ObjcBridge").join("ObjcBridge.h"),
            "#import <Foundation/Foundation.h>\n\n@interface MGShared : NSObject\n+ (NSString *)hello;\n@end\n",
        )?;
        write_file(
            &target.join("ios").join("ObjcBridge").join("ObjcBridge.m"),
            "#import \"ObjcBridge.h\"\n\n@implementation MGShared\n+ (NSString *)hello {\n    return @\"hello from MegaGate shared (objc)\";\n}\n@end\n",
        )?;

        // react-native/ — js entry (scripts.android/ios: `mg dev` chạy qua npm run — C9)
        write_file(
            &target.join("react-native").join("package.json"),
            &format!(
                "{{\n  \"name\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"private\": true,\n  \"scripts\": {{\n    \"android\": \"react-native run-android\",\n    \"ios\": \"react-native run-ios\"\n  }},\n  \"dependencies\": {{\n    \"react\": \"18.2.0\",\n    \"react-native\": \"0.74.0\"\n  }}\n}}\n",
                slug
            ),
        )?;
        write_file(
            &target.join("react-native").join("index.js"),
            "const { AppRegistry } = require('react-native');\nconst App = require('./App');\n\nAppRegistry.registerComponent('main', () => App);\n",
        )?;
        write_file(
            &target.join("react-native").join("App.js"),
            "const React = require('react');\nconst { Text } = require('react-native');\n\nmodule.exports = function App() {\n  return React.createElement(Text, null, 'hello from MegaGate (react-native)');\n};\n",
        )?;

        // flutter/ — dart entry
        write_file(
            &target.join("flutter").join("pubspec.yaml"),
            &format!(
                "name: {}\ndescription: MegaGate Flutter platform entry\ndependencies:\n  flutter:\n    sdk: flutter\n",
                slug
            ),
        )?;
        write_file(
            &target.join("flutter").join("lib").join("main.dart"),
            "import 'package:flutter/material.dart';\n\nvoid main() => runApp(const App());\n\nclass App extends StatelessWidget {\n  const App({super.key});\n\n  @override\n  Widget build(BuildContext context) {\n    return MaterialApp(home: Scaffold(body: Center(child: Text('hello from MegaGate (flutter)'))));\n  }\n}\n",
        )?;

        Ok(())
    }
}
