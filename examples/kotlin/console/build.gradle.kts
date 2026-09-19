plugins {
    application
    kotlin("jvm") version "2.4.20"
}

kotlin {
    jvmToolchain(17)
}

sourceSets {
    main {
        java.srcDir(rootProject.file("../dist/java"))
        resources {
            srcDir(rootProject.file("../dist/java"))
            include("native/**")
        }
    }
}

application {
    mainClass = "dev.whalefrommars.examples.kotlin.MainKt"
}
