inherit("MrxMissionFlow")
import("MrxCinematic")
import("MrxFactionManager")
import("MrxGui")
import("MrxGuiBase")
import("MrxLayerManager")
import("MrxPlayState")
import("MrxStarterManager")
import("MrxState")
import("MrxSupportData")
import("MrxUtil")
import("MrxTransit")
import("MrxVoSequence")
import("Munitions")
import("WifVzBoundary")
import("WifPmcInterior")
import("MrxAchievements")
import("MrxVerifyManager")
import("WifHints")
import("MrxSoundBootstrap")
import("WifBios")
import("WifPmcInterior")
import("MrxSoundCategories")
import("MrxMusic")
import("friendlygate")

function GetOriginalFlowData()
  local sHeroLetter = MrxUtil.GetCharacterIdentity(Player.GetPrimaryCharacter())
  sHeroLetter = sHeroLetter and string.upper(string.sub(sHeroLetter, 1, 1))
  if sHeroLetter ~= "M" and sHeroLetter ~= "J" and sHeroLetter ~= "C" then
    sHeroLetter = "M"
  end
  return {
    Start = {
      fPrereq = function()
        return true
      end,
      fConseq = function()
        _BeginBlockingSequence()
        
        local function c()
          UnlockMission("VzaCon001")
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
          else
            _EndBlockingSequence()
          end
        end
        
        local function a()
          WifBios.AddDossierEntry("BioChris")
          WifBios.AddDossierEntry("BioJennifer")
          WifBios.AddDossierEntry("BioMattias")
          WifBios.AddDossierEntry("BioFiona")
          WifBios.AddDossierEntry("BioSolano")
          WifBios.AddDossierEntry("BioBlanco")
          WifBios.AddDossierEntry("BioCarmona")
          SetGrappleEnabled(false)
          SetVehicleDisguiseEnabled(false)
          EnableResourceCounters(false)
          MrxSupportData.SetHeliPilotRecruited(false)
          MrxSupportData.SetMechanicRecruited(false)
          MrxSupportData.SetJetPilotRecruited(false)
          _PlayMovie({
            sMovie = "01_AOA_" .. sHeroLetter,
            fCallback = c,
            bSubtitles = true
          })
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    VzaCon001 = {
      fPrereq = function()
        return HasKey("VzaCon001")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        
        local function d()
          UnlockMission("PmcCon001")
          MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.VZCon01")
          _EndBlockingSequence()
        end
        
        local function c()
          WifVzBoundary.SetupBoundary00()
          local tStartLocations = GetMissionStartLocations("PmcCon001")
          if type(tStartLocations) == "table" then
            MrxUtil.TeleportHeroesToLocations(tStartLocations, d)
          end
        end
        
        local function a()
          EnableResourceCounters(true)
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_RAGE")
          _PlayMovie({
            sMovie = "02_AOB_" .. sHeroLetter,
            fCallback = c,
            bSubtitles = true
          })
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    PmcCon001 = {
      fPrereq = function()
        return HasKey("PmcCon001")
      end,
      fConseq = function()
        WifBios.AddDossierEntry("BioUP")
        WifBios.AddDossierEntry("BioRubin")
        _BeginBlockingSequence()
        WifHints.AddActiveHint("FionaHint25")
        
        local function d()
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
          else
            _EndBlockingSequence()
          end
        end
        
        local function c()
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_HELLOHURRAY")
          WifVzBoundary.SetupBoundaryINTRO_OIL()
          local uOilHqGate = Pg.GetGuidByName("_ocoutpost_wallgate 0x000f9a64")
          if uOilHqGate then
            friendlygate.LockGate(uOilHqGate, true)
          end
          UnlockMission("OilCon020")
          UnlockMission("PmcCon031")
          UnlockMission("PmcJob001")
          WifPmcInterior.Unlock()
          WifPmcInterior.SetTeleportCallback(d)
          WifPmcInterior.Enter(true)
        end
        
        local function b()
          MrxLayerManager.MarkForRemoval("Vz_State_Pmc_Pristine")
          local tLayers = {
            "Vz_State_Pmc_LivedIn",
            "vz_state_gur_fuel_junglemountains",
            "vz_state_oil_fuel_maroutskirts",
            "vz_state_gurcon005_airportdefbase_staging",
            "vz_state_pmc_act1"
          }
          MrxLayerManager.MarkForAddition(tLayers)
          MrxLayerManager.ProcessMarkedLayers(c)
        end
        
        local function a()
          _PlayMovie({
            sMovie = "06_YNH_" .. sHeroLetter,
            fCallback = b,
            bSubtitles = true
          })
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    OilCon020 = {
      fPrereq = function()
        return HasKey("OilCon020")
      end,
      fConseq = function()
        UnlockMission("OilCon021")
        WifHints.RemoveActiveHint("FionaHint25")
        WifHints.AddActiveHint("FionaHint21")
        MrxAchievements.AchievementAddCount("ACHIEVEMENT_GUN_RUNNER", 1)
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxVoSequence.Start({
            "Fiona-In-Mission-Freeplay-None-21"
          }, nil, MrxVoSequence.knPriorityFreeplay)
        end
      end
    },
    OilCon021 = {
      fPrereq = function()
        return HasKey("OilCon021")
      end,
      fConseq = function()
        WifHints.AddActiveHint("FionaHint22")
        MrxStarterManager.DestroyStarter("OilStarter5")
        MrxFactionManager.SetAttitudeMutable("Oil")
        local uOilHqGate = Pg.GetGuidByName("_ocoutpost_wallgate 0x000f9a64")
        if uOilHqGate then
          friendlygate.LockGate(uOilHqGate, false)
        end
        SetVehicleDisguiseEnabled(true)
        UnlockMission("OilCon002")
        MrxLayerManager.MarkForRemoval("vz_state_OilCon021_staging")
      end
    },
    OilJob011 = {
      fPrereq = function()
        return HasKey("OilJob011")
      end,
      fConseq = function()
        MrxVerifyManager.SetKilledIfNotSet("OilJob011_Target_01")
        MrxVerifyManager.SetKilledIfNotSet("OilJob011_Target_02")
        MrxVerifyManager.SetKilledIfNotSet("OilJob011_Target_03")
        MrxVerifyManager.SetKilledIfNotSet("OilJob011_Target_04")
        MrxVerifyManager.SetKilledIfNotSet("OilJob011_Target_05")
        MrxVerifyManager.SetKilledIfNotSet("OilJob012_Target_01")
        MrxVerifyManager.SetKilledIfNotSet("OilJob012_Target_02")
        MrxVerifyManager.SetKilledIfNotSet("OilJob012_Target_03")
        MrxVerifyManager.SetKilledIfNotSet("OilJob012_Target_04")
        MrxVerifyManager.SetKilledIfNotSet("OilJob012_Target_05")
      end
    },
    PmcCon002 = {
      fPrereq = function()
        return HasKey("PmcCon002")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        
        local function c()
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
          else
            _EndBlockingSequence()
          end
        end
        
        local function b()
          WifPmcInterior.SetTeleportCallback(c)
          WifPmcInterior.Enter(true)
        end
        
        local function a()
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_RUNNING_WITH_DEVIL")
          MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.PMC02")
          WifHints.RemoveActiveHint("FionaHint10")
          MrxVerifyManager.SetKilledIfNotSet("PmcCon002 Blanco")
          _PlayMovie({
            sMovie = "11_SR1_S",
            fCallback = b,
            bSubtitles = true
          })
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    PmcCon002_NotMecIntro = {
      fPrereq = function()
        return HasKey("PmcCon002") and not HasKey("MecIntro")
      end,
      fConseq = function()
        WifBios.AddDossierEntry("BioEva")
        WifHints.RemoveActiveHint("FionaHint18")
        WifHints.AddActiveHint("FionaHint30")
      end
    },
    PmcCon002_MecIntro = {
      fPrereq = function()
        return HasKey("PmcCon002") and HasKey("MecIntro") and not HasKey("MecCon001")
      end,
      fConseq = function()
        WifHints.RemoveActiveHint("FionaHint30")
        WifHints.AddActiveHint("FionaHint26")
      end
    },
    PmcCon002_MecCon001 = {
      fPrereq = function()
        return HasKey("PmcCon002") and HasKey("MecCon001") and not HasKey("JetIntro")
      end,
      fConseq = function()
        WifHints.RemoveActiveHint("FionaHint26")
        WifHints.AddActiveHint("FionaHint32")
      end
    },
    PmcCon002_JetIntro = {
      fPrereq = function()
        return HasKey("PmcCon002") and HasKey("JetIntro") and not HasKey("JetCon001")
      end,
      fConseq = function()
        WifHints.AddActiveHint("FionaHint29")
      end
    },
    PmcCon002_JetCon001 = {
      fPrereq = function()
        return HasKey("PmcCon002") and HasKey("JetCon001")
      end,
      fConseq = function()
        WifHints.AddActiveHint("FionaHint28")
        UnlockMission("PmcCon003")
      end
    },
    PmcCon003 = {
      fPrereq = function()
        return HasKey("PmcCon003")
      end,
      fConseq = function()
        AwardKey("Invasion")
        MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.Invasion01")
        _BeginBlockingSequence()
        
        local function b()
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
          else
            _EndBlockingSequence()
          end
          WifVzBoundary.SetupBoundary02()
        end
        
        local function a()
          WifHints.RemoveActiveHint("FionaHint28")
          WifHints.AddActiveHint("FionaHint33")
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_BETTER_RUN")
          MrxVerifyManager.SetKilledIfNotSet("CarmonaTarget")
          _AddIntro("PmcBoss", "AllChi")
          MrxLayerManager.MarkForRemoval({
            "vz_state_amazon_act1",
            "vz_state_mar_industrial_act2",
            "vz_state_mar_city_pristine",
            "vz_state_mar_city_act1",
            "vz_state_mar_city_act1_depot"
          })
          MrxLayerManager.MarkForAddition({
            "vz_state_All_Hq_Structures",
            "vz_state_staging_all_HQ",
            "vz_state_staging_chi_HQ",
            "vz_state_cumana_act1ALL_N",
            "vz_state_cumana_act1ALL_S",
            "vz_state_cumana_act1ALL_staging",
            "vz_state_cumana_act1CHI",
            "vz_state_amazon_act2",
            "vz_state_margarita_act2",
            "vz_state_mar_industrial_act3",
            "vz_state_mar_city_ruined",
            "vz_state_mar_city_act2",
            "vz_state_car_dock_act1",
            "vz_state_car_estate_act1",
            "vz_state_car_big_lineregion",
            "vz_state_car_shanty_act1",
            "vz_state_All_fuel_amazon",
            "vz_state_chi_fuel_amazon",
            "vz_state_car_city_act1",
            "vz_state_vza_con001_post",
            "Vz_State_AllJob005_01",
            "Vz_State_AllJob005_02",
            "Vz_State_AllJob005_03",
            "Vz_State_AllJob005_04",
            "Vz_State_AllJob005_05",
            "Vz_State_AllJob005_01_Staging",
            "Vz_State_AllJob005_02_Staging",
            "Vz_State_AllJob005_03_Staging",
            "Vz_State_AllJob005_04_Staging",
            "Vz_State_AllJob005_05_Staging",
            "Vz_State_AllJob009_01_Staging",
            "Vz_State_AllJob009_02_Staging",
            "Vz_State_AllJob009_03_Staging",
            "Vz_State_AllJob009_04_Staging",
            "Vz_State_AllJob009_05_Staging",
            "Vz_State_AllJob010_01_Staging",
            "Vz_State_AllJob010_02_Staging",
            "Vz_State_AllJob010_03_Staging",
            "Vz_State_AllJob010_04_Staging",
            "Vz_State_AllJob010_05_Staging",
            "Vz_State_ChiCon005_a_Pristine",
            "Vz_State_ChiCon005_b_Pristine",
            "Vz_State_ChiCon005_c_Pristine",
            "Vz_State_ChiCon006_a_Pristine",
            "Vz_State_ChiCon006_b_Pristine",
            "Vz_State_ChiCon006_c_Pristine",
            "Vz_State_ChiJob005_A_Staging",
            "Vz_State_ChiJob005_B_Staging",
            "Vz_State_ChiJob005_C_Staging",
            "Vz_State_ChiJob005_D_Staging",
            "Vz_State_ChiJob005_E_Staging",
            "Vz_State_ChiJob005_F_Staging",
            "Vz_State_ChiJob005_G_Staging",
            "Vz_State_ChiJob009_A_Staging",
            "Vz_State_ChiJob010_01_Staging",
            "Vz_State_ChiJob010_02_Staging",
            "Vz_State_ChiJob010_03_Staging",
            "Vz_State_ChiJob010_04_Staging",
            "Vz_State_ChiJob010_05_Staging",
            "Vz_State_PirJob012_01_Pristine",
            "Vz_State_PirJob012_01_Staging",
            "Vz_State_PirJob012_02_Staging",
            "Vz_State_PirJob012_03_Pristine",
            "Vz_State_PirJob012_03_Staging",
            "Vz_State_PirJob012_04_Staging"
          })
          _PlayMovie({
            sMovie = "11_SR2_S",
            fCallback = b,
            bSubtitles = true
          })
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    AllChiIntro = {
      fPrereq = function()
        return HasKey("AllChiIntro")
      end,
      fConseq = function()
        WifBios.AddDossierEntry("BioAllies")
        WifBios.AddDossierEntry("BioChina")
        WifBios.AddDossierEntry("BioJoyce")
        WifBios.AddDossierEntry("BioPeng")
        MrxFactionManager.SetAttitudeMutable("All")
        MrxFactionManager.SetAttitudeMutable("Chi")
        UnlockMission("AllCon050")
        UnlockMission("ChiCon050")
      end
    },
    PmcCon004 = {
      fPrereq = function()
        return HasKey("PmcCon004")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        
        local function CleanupStarters()
          DestroyMission("AllCon001")
          DestroyMission("AllCon002")
          DestroyMission("AllCon003")
          DestroyMission("AllCon008")
          DestroyMission("AllCon050")
          DestroyMission("ChiCon001")
          DestroyMission("ChiCon002")
          DestroyMission("ChiCon003")
          DestroyMission("ChiCon008")
          DestroyMission("ChiCon050")
          DestroyMission("GurCon003")
          MrxStarterManager.DestroyStarter("AllStarter0")
          MrxStarterManager.DestroyStarter("ChiStarter0")
        end
        
        local function e(oCredits)
          if Sys.ForceNextAutosave then
            Sys.ForceNextAutosave()
          end
          MrxMusic.StopSpecialMusic("silence")
          MrxSoundCategories.Fade("credits", false)
          oCredits:SetSwfFile(nil)
          MrxGuiBase.ReleaseControlFocus(oCredits)
          MrxGui.RemoveWidget(oCredits)
          oCredits:delete()
          MrxGui.FadeFromColor(0)
          _EndBlockingSequence()
          Sys.RequestGameState("unloading")
          Net.QuitGame()
        end
        
        local function c(oCredits)
          MrxMusic.PlaySpecialMusic("mu_maintheme")
          oCredits:SetFlashEventHandler("creditsEnd", e, {oCredits})
          oCredits:SetEventHandler("ControllerInput", e)
          MrxGuiBase.GetControlFocus(oCredits)
        end
        
        local function b()
          MrxGui.FadeToColor(0)
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME)
          end
          Sys.RequestGameState("cinematic")
          local oCredits = MrxGui.FlashWidget:new()
          oCredits:SetFullscreen(true)
          oCredits:SetSwfFile("credits", c, {oCredits})
          MrxGui.AddWidget(oCredits)
          if Net.IsServer() then
            Net.SendCustomEvent("WifMissionFlow", NETEVENT_CLIENTCREDITS, {})
          end
        end
        
        local function a()
          WifHints.RemoveActiveHint("FionaHint06")
          MrxVerifyManager.SetSolanoVerified()
          local sMovie = "15_ACK_" .. sHeroLetter
          CleanupStarters()
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_HERO_AND_MADMAN")
          if MrxVerifyManager.CheckTechnoVikingAchievement() then
            MrxAchievements.NetGrantAchievement("ACHIEVEMENT_TECHNO_VIKING", Player.GetPrimaryPlayer())
          end
          MrxSoundCategories.Fade("credits", true)
          _PlayMovie({
            sMovie = sMovie,
            fCallback = b,
            bSubtitles = true
          })
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    PmcCon016 = {
      fPrereq = function()
        return HasKey("PmcCon016")
      end,
      fConseq = function()
        UnlockMission("PmcCon015")
      end
    },
    PmcCon031 = {
      fPrereq = function()
        return HasKey("PmcCon031")
      end,
      fConseq = function()
        UnlockMission("PmcCon032")
      end
    },
    PmcCon032 = {
      fPrereq = function()
        return HasKey("PmcCon032")
      end,
      fConseq = function()
        UnlockMission("PmcCon033")
      end
    },
    PmcCon031_x3 = {
      fPrereq = function()
        return GetKeyValue("PmcCon031") >= 3
      end,
      fConseq = _AddHeroCostume
    },
    PmcCon032_x3 = {
      fPrereq = function()
        return GetKeyValue("PmcCon032") >= 3
      end,
      fConseq = _AddHeroCostume
    },
    PmcCon033_x3 = {
      fPrereq = function()
        return GetKeyValue("PmcCon033") >= 3
      end,
      fConseq = _AddHeroCostume
    },
    PmcCon034_x3 = {
      fPrereq = function()
        return GetKeyValue("PmcCon034") >= 3
      end,
      fConseq = _AddHeroCostume
    },
    MecCon001 = {
      fPrereq = function()
        return HasKey("MecCon001")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        
        local function c()
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
          else
            _EndBlockingSequence()
          end
        end
        
        local function b()
          MrxStarterManager.DestroyStarter("MecBoss")
          WifHints.AddActiveHint("FionaHint16")
          WifHints.UnlockAllHints("Eva")
          UnlockMission("PmcCon016")
          SetGrappleEnabled(true)
          _RemoveIntro("HelPmcBoss", "Mec")
          _AddIntro("MecPmcBoss", "Jet")
          WifPmcInterior.SetTeleportCallback(c)
          WifPmcInterior.Enter(true)
        end
        
        local function a()
          _PlayMovie({
            sMovie = "08_RME_" .. sHeroLetter,
            fCallback = b,
            bSubtitles = true
          })
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_OIL_AND_GAZ")
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    JetIntro = {
      fPrereq = function()
        return HasKey("JetIntro")
      end,
      fConseq = function()
        WifBios.AddDossierEntry("BioMisha")
        WifHints.RemoveActiveHint("FionaHint32")
        WifHints.RemoveActiveHint("FionaHint16")
        WifHints.AddActiveHint("FionaHint17")
        UnlockMission("JetCon001")
        WifVzBoundary.SetupBoundaryPOST_EVA_POST_PIR()
        WifVzBoundary.SetInteriorMode(true)
      end
    },
    JetCon001 = {
      fPrereq = function()
        return HasKey("JetCon001")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        
        local function c()
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
          else
            _EndBlockingSequence()
          end
        end
        
        local function b()
          _RemoveIntro("MecPmcBoss", "Jet")
          WifHints.RemoveActiveHint("FionaHint29")
          WifHints.RemoveActiveHint("FionaHint17")
          WifHints.UnlockAllHints("Misha")
          MrxStarterManager.DestroyStarter("JetBoss")
          UnlockMission("PmcCon018")
          WifPmcInterior.SetTeleportCallback(c)
          WifPmcInterior.Enter(true)
        end
        
        local function a()
          MrxLayerManager.MarkForRemoval({
            "Vz_State_JetCon001_Pristine",
            "Vz_State_JetCon001_CP01"
          })
          MrxLayerManager.MarkForAddition("vz_state_JetCon001_Post")
          _PlayMovie({
            sMovie = "09_RJE_" .. sHeroLetter,
            fCallback = b,
            bSubtitles = true
          })
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_ANALOG_KID")
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    PmcJob001 = {
      fPrereq = function()
        return HasKey("PmcJob001")
      end
    },
    OilCon002 = {
      fPrereq = function()
        return HasKey("OilCon002")
      end,
      fConseq = function()
        MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.Oil02")
        _BeginBlockingSequence()
        
        local function b()
          WifHints.RemoveActiveHint("FionaHint21")
          WifHints.RemoveActiveHint("FionaHint22")
          WifHints.AddActiveHint("FionaHint20")
          WifHints.AddActiveHint("FionaHint19")
          WifHints.AddActiveHint("FionaHint18")
          WifHints.UnlockAllHints("Ewan")
          Munitions.SetMunitionsTaggable(true)
          MrxTransit.SetSystemEnabled(true)
          MrxTransit.SetLocationEnabled(1, "Pmc", true)
          MrxTransit.SetLocationEnabled(2, "Oil", true)
          local oOilHq = MrxHqManager.GetHq("OilHq")
          oOilHq:RefreshUiDisplay()
          MrxUnlockFanfare.AddUnlockedItem({sType = "bounty", sFactionId = "Oil"})
          UnlockMission("OilCon050")
          UnlockMission("OilJob004")
          UnlockMission("OilJob008")
          UnlockMission("OilJob011")
          UnlockMission("PmcCon013")
          UnlockMission("PmcCon034")
          MrxLayerManager.MarkForRemoval("vz_state_gurcon005_airportdefbase_staging")
          MrxLayerManager.MarkForAddition({
            "vz_state_gurcon005_airportdefbase",
            "vz_state_mar_city_act1_depot"
          })
          WifVzBoundary.SetupBoundaryPOST_OIL()
          _AddIntro("PmcBoss", "Gur")
          _AddIntro("HelPmcBoss", "Mec")
          WifPmcInterior.Enter(true)
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
          else
            _EndBlockingSequence()
          end
        end
        
        local function a()
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_WILD_ONE")
          _PlayMovie({
            sMovie = "07_RHE_" .. sHeroLetter,
            fCallback = b,
            bSubtitles = true
          })
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    GurIntro = {
      fPrereq = function()
        return HasKey("GurIntro")
      end,
      fConseq = function()
        WifBios.AddDossierEntry("BioAcosta")
        WifBios.AddDossierEntry("BioPLAV")
        MrxFactionManager.SetAttitudeMutable("Gur")
        WifHints.RemoveActiveHint("FionaHint19")
        WifHints.AddActiveHint("FionaHint09")
        UnlockMission("GurCon053")
        if not HasKey("MecIntro") then
          WifVzBoundary.SetupBoundaryPOST_EVA_PRE_PIR()
          WifVzBoundary.SetInteriorMode(true)
        end
      end
    },
    MecIntro = {
      fPrereq = function()
        return HasKey("MecIntro")
      end,
      fConseq = function()
        WifHints.RemoveActiveHint("FionaHint18")
        UnlockMission("MecCon001")
        AwardKey("MonsterV4")
        if not HasKey("GurIntro") then
          WifVzBoundary.SetupBoundaryPOST_EVA_PRE_PIR()
          WifVzBoundary.SetInteriorMode(true)
        end
      end
    },
    OilCon050 = {
      fPrereq = function()
        return HasKey("OilCon050")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        WifHints.RemoveActiveHint("FionaHint20")
        WifHints.AddActiveHint("FionaHint23")
        WifHints.AddActiveHint("FionaHint24")
        UnlockMission("OilCon001")
        UnlockMission("OilCon051")
        MrxLayerManager.MarkForRemoval("vz_state_mar_altagracia_act1")
        MrxLayerManager.MarkForAddition("vz_state_mar_altagracia_act2")
        _ChangeOutpostStaging(_EndBlockingSequence)
        if not HasKey("GurCon053") and not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxVoSequence.Start({
            "Fiona.Misc.Outposts24"
          }, nil, MrxVoSequence.knPriorityFreeplay)
        end
      end
    },
    OilCon001 = {
      fPrereq = function()
        return HasKey("OilCon001")
      end,
      fConseq = function()
        MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.Oil01")
        WifHints.RemoveActiveHint("FionaHint23")
        WifHints.RemoveActiveHint("FionaHint24")
        MrxLayerManager.MarkForRemoval("vz_state_mar_industrial_act1")
        MrxLayerManager.MarkForAddition({
          "vz_state_mar_industrial_act2",
          "vz_state_oilcon001_post"
        })
      end
    },
    OilCon001_GurCon001 = {
      fPrereq = function()
        return HasKey("OilCon001") and HasKey("GurCon001")
      end,
      fConseq = function()
        WifHints.AddActiveHint("FionaHint10")
        UnlockMission("PmcCon002")
      end
    },
    OilCon051 = {
      fPrereq = function()
        return HasKey("OilCon051")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        UnlockMission("OilCon003")
        UnlockMission("OilCon052")
        MrxLayerManager.MarkForRemoval("vz_state_mar_outskirt_act1")
        MrxLayerManager.MarkForAddition("vz_state_mar_outskirt_act2")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    OilCon052 = {
      fPrereq = function()
        return HasKey("OilCon052")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        if not HasKey("ChiCon002") then
          UnlockMission("OilCon005")
        end
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    OilOutposts = {
      fPrereq = function()
        return HasKey("OilCon050") and HasKey("OilCon051") and HasKey("OilCon052")
      end,
      fConseq = function()
        MrxAchievements.NetGrantAchievement("ACHIEVEMENT_PIPELINE")
      end
    },
    GurCon053 = {
      fPrereq = function()
        return HasKey("GurCon053")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        WifHints.RemoveActiveHint("FionaHint09")
        WifHints.AddActiveHint("FionaHint14")
        WifHints.AddActiveHint("FionaHint15")
        MrxUnlockFanfare.AddUnlockedItem({sType = "bounty", sFactionId = "Gur"})
        UnlockMission("GurCon002")
        UnlockMission("GurCon050")
        UnlockMission("GurJob001")
        UnlockMission("GurJob002")
        UnlockMission("GurJob006")
        UnlockMission("GurJob020")
        if not HasKey("OilCon050") and not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxVoSequence.Start({
            "Fiona.Misc.Outposts24"
          }, nil, MrxVoSequence.knPriorityFreeplay)
        end
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    GurCon002 = {
      fPrereq = function()
        return HasKey("GurCon002")
      end,
      fConseq = function()
        WifHints.RemoveActiveHint("FionaHint14")
        WifHints.RemoveActiveHint("FionaHint15")
        WifHints.AddActiveHint("FionaHint11")
        WifHints.AddActiveHint("FionaHint12")
        UnlockMission("GurCon003")
        MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.Gur02")
        MrxLayerManager.MarkForRemoval({
          "vz_state_merida_act1",
          "vz_state_merida_act1_helo",
          "vz_state_merida_act1_staging"
        })
        MrxLayerManager.MarkForAddition({
          "vz_state_merida_act2",
          "vz_state_merida_act2_helo"
        })
        MrxVerifyManager.SetKilledIfNotSet("Mendez")
        _RemoveIntro("PmcBoss", "Gur")
        _AddIntro("PmcBoss", "Pir")
        WifVzBoundary.SetupBoundaryPOST_EVA_POST_PIR()
      end
    },
    PirIntro = {
      fPrereq = function()
        return HasKey("PirIntro")
      end,
      fConseq = function()
        WifBios.AddDossierEntry("BioDevilbwoy")
        WifBios.AddDossierEntry("BioPirates")
        MrxFactionManager.SetAttitudeMutable("Pir")
        UnlockMission("PirCon001")
      end
    },
    GurCon003 = {
      fPrereq = function()
        return HasKey("GurCon003")
      end,
      fConseq = function()
        WifHints.RemoveActiveHint("FionaHint12")
        WifHints.AddActiveHint("FionaHint13")
        UnlockMission("GurCon001")
        MrxLayerManager.MarkForRemoval("vz_state_jungle_mountain_act1")
        MrxLayerManager.MarkForAddition("vz_state_jungle_mountain_act2")
      end
    },
    GurCon001 = {
      fPrereq = function()
        return HasKey("GurCon001")
      end,
      fConseq = function()
        WifHints.RemoveActiveHint("FionaHint11")
        WifHints.RemoveActiveHint("FionaHint13")
        MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.Gur01")
        MrxLayerManager.MarkForRemoval({
          "vz_state_gurcon001_fortress",
          "vz_state_gurcon001_staging",
          "Vz_State_GurCon001"
        })
        MrxLayerManager.MarkForAddition("vz_state_gurcon001_fortress_destroyed")
      end
    },
    GurCon050 = {
      fPrereq = function()
        return HasKey("GurCon050")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        UnlockMission("GurCon005")
        UnlockMission("GurCon052")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    GurCon052 = {
      fPrereq = function()
        return HasKey("GurCon052")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        MrxStarterManager.RequestStarter("GurStarter4")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    GurOutposts = {
      fPrereq = function()
        return HasKey("GurCon050") and HasKey("GurCon052") and HasKey("GurCon053")
      end,
      fConseq = function()
        MrxAchievements.NetGrantAchievement("ACHIEVEMENT_PIPELINE")
      end
    },
    GurJob002 = {
      fPrereq = function()
        return HasKey("GurJob002")
      end,
      fConseq = function()
        MrxVerifyManager.SetKilledIfNotSet("GurJob002_01_Target")
        MrxVerifyManager.SetKilledIfNotSet("GurJob002_02_Target")
        MrxVerifyManager.SetKilledIfNotSet("GurJob002_03_Target")
        MrxVerifyManager.SetKilledIfNotSet("GurJob002_04_Target")
        MrxVerifyManager.SetKilledIfNotSet("GurJob002_05_Target")
        MrxVerifyManager.SetKilledIfNotSet("GurJob012_01_Target")
        MrxVerifyManager.SetKilledIfNotSet("GurJob012_02_Target")
        MrxVerifyManager.SetKilledIfNotSet("GurJob012_03_Target")
        MrxVerifyManager.SetKilledIfNotSet("GurJob012_04_Target")
        MrxVerifyManager.SetKilledIfNotSet("GurJob012_05_Target")
      end
    },
    AllCon002 = {
      fPrereq = function()
        return HasKey("AllCon002")
      end,
      fConseq = function()
        UnlockMission("AllCon001")
        WifHints.AddActiveHint("FionaHint01")
        MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.All02")
      end
    },
    AllCon001 = {
      fPrereq = function()
        return HasKey("AllCon001")
      end,
      fConseq = function()
        UnlockMission("AllCon003")
        WifHints.RemoveActiveHint("FionaHint03")
        MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.All01")
        MrxLayerManager.MarkForRemoval({
          "vz_state_car_city_act1",
          "vz_state_car_estate_act1",
          "vz_state_car_dock_act1",
          "vz_state_car_city_act2chi",
          "vz_state_car_city_act2chi_staging"
        })
        MrxLayerManager.MarkForAddition({
          "vz_state_car_city_act2all",
          "vz_state_car_city_act2all_staging"
        })
      end
    },
    AllCon003 = {
      fPrereq = function()
        return HasKey("AllCon003")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        
        local function c()
          WifVzBoundary.EnableExclusionBoundary("Boundary_ALHQ_Exclusion", true)
          WifVzBoundary.DrawExclusionBoundaryOnMap("Boundary_ALHQ_Exclusion", true)
          MrxTransit.SetLocationIsNuked(7, true)
          UnlockMission("PmcCon004")
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
          else
            _EndBlockingSequence()
          end
        end
        
        local function b()
          local tStartLocations = GetMissionStartLocations("PmcCon004")
          if type(tStartLocations) == "table" then
            MrxUtil.TeleportHeroesToLocations(tStartLocations, c)
          end
        end
        
        local function a()
          WifHints.RemoveActiveHint("FionaHint01")
          WifHints.RemoveActiveHint("FionaHint02")
          WifHints.RemoveActiveHint("FionaHint04")
          WifHints.RemoveActiveHint("FionaHint05")
          WifHints.RemoveActiveHint("FionaHint07")
          WifHints.RemoveActiveHint("FionaHint08")
          WifHints.AddActiveHint("FionaHint06")
          MrxVerifyManager.SetKilledIfNotSet("AllCon003_HVT")
          MrxLayerManager.MarkForRemoval({
            "vz_state_car_city_act1",
            "vz_state_car_city_act2all",
            "vz_state_car_city_act2all_staging",
            "vz_state_car_city_act2chi_staging",
            "vz_state_car_city_act2chi",
            "vz_state_car_city_act3chi"
          })
          MrxLayerManager.MarkForAddition({
            "vz_state_PmcCon004_AlliesNuked",
            "vz_state_car_city_act3all"
          })
          MrxLayerManager.ProcessMarkedLayers()
          _PlayMovie({
            sMovie = "13_AVI_" .. sHeroLetter,
            fCallback = b,
            bSubtitles = true
          })
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_NO_COMPROMISE")
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    AllCon050 = {
      fPrereq = function()
        return HasKey("AllCon050")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        WifHints.RemoveActiveHint("FionaHint33")
        WifHints.AddActiveHint("FionaHint03")
        if not HasKey("ChiCon050") then
          WifHints.AddActiveHint("FionaHint04")
        end
        MrxUnlockFanfare.AddUnlockedItem({sType = "bounty", sFactionId = "All"})
        UnlockMission("AllCon002")
        UnlockMission("AllCon052")
        UnlockMission("AllJob002")
        UnlockMission("AllJob003")
        UnlockMission("AllJob020")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    AllCon052 = {
      fPrereq = function()
        return HasKey("AllCon052")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        if not HasKey("PmcCon004") then
          UnlockMission("AllCon008")
        end
        UnlockMission("AllCon053")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    AllCon053 = {
      fPrereq = function()
        return HasKey("AllCon053")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        MrxStarterManager.RequestStarter("AllStarter4")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    AllOutposts = {
      fPrereq = function()
        return HasKey("AllCon050") and HasKey("AllCon052") and HasKey("AllCon053")
      end,
      fConseq = function()
        MrxAchievements.NetGrantAchievement("ACHIEVEMENT_PIPELINE")
      end
    },
    AllJob002 = {
      fPrereq = function()
        return HasKey("AllJob002")
      end,
      fConseq = function()
        MrxVerifyManager.SetKilledIfNotSet("AllJob002_01_Target")
        MrxVerifyManager.SetKilledIfNotSet("AllJob002_02_Target")
        MrxVerifyManager.SetKilledIfNotSet("AllJob002_03_Target")
        MrxVerifyManager.SetKilledIfNotSet("AllJob002_04_Target")
        MrxVerifyManager.SetKilledIfNotSet("AllJob002_05_Target")
        MrxVerifyManager.SetKilledIfNotSet("AllJob010_01_Target")
        MrxVerifyManager.SetKilledIfNotSet("AllJob010_02_Target")
        MrxVerifyManager.SetKilledIfNotSet("AllJob010_03_Target")
        MrxVerifyManager.SetKilledIfNotSet("AllJob010_04_Target")
        MrxVerifyManager.SetKilledIfNotSet("AllJob010_05_Target")
      end
    },
    AllCon050_ChiCon050 = {
      fPrereq = function()
        return HasKey("AllCon050") and HasKey("ChiCon050")
      end,
      fConseq = function()
        WifHints.RemoveActiveHint("FionaHint07")
        WifHints.RemoveActiveHint("FionaHint04")
        WifHints.AddActiveHint("FionaHint02")
      end
    },
    AllCon001_ChiCon001 = {
      fPrereq = function()
        return HasKey("AllCon001") and HasKey("ChiCon001")
      end,
      fConseq = function()
        _RemoveIntro("PmcBoss", "AllChi")
      end
    },
    ChiCon050 = {
      fPrereq = function()
        return HasKey("ChiCon050")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        WifHints.RemoveActiveHint("FionaHint33")
        if not HasKey("AllCon050") then
          WifHints.AddActiveHint("FionaHint07")
        end
        WifHints.AddActiveHint("FionaHint08")
        MrxUnlockFanfare.AddUnlockedItem({sType = "bounty", sFactionId = "Chi"})
        UnlockMission("ChiCon001")
        UnlockMission("ChiCon051")
        UnlockMission("ChiJob002")
        UnlockMission("ChiJob003")
        UnlockMission("ChiJob020")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    ChiCon001 = {
      fPrereq = function()
        return HasKey("ChiCon001")
      end,
      fConseq = function()
        UnlockMission("ChiCon002")
        WifHints.RemoveActiveHint("FionaHint08")
        WifHints.AddActiveHint("FionaHint05")
        MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.Chi01")
        MrxLayerManager.MarkForRemoval({
          "vz_state_cumana_act1ALL_N",
          "vz_state_cumana_act1ALL_S",
          "vz_state_cumana_act1ALL_staging",
          "vz_state_cumana_act1CHI"
        })
        MrxLayerManager.MarkForAddition("vz_state_cumana_act2chi")
      end
    },
    ChiCon002 = {
      fPrereq = function()
        return HasKey("ChiCon002")
      end,
      fConseq = function()
        DestroyMission("OilCon005")
        UnlockMission("ChiCon003")
        MrxSoundBootstrap.SetPmcRadio("ReporterNeutral.MissionVO.Chi02")
        MrxLayerManager.MarkForRemoval({
          "vz_state_mar_city_act2",
          "vz_state_staging_OilDepot",
          "vz_state_staging_OilHQ",
          "vz_state_car_city_act1",
          "vz_state_car_city_all",
          "vz_state_car_estate_act1",
          "vz_state_car_dock_act1",
          "vz_state_car_city_act2all",
          "vz_state_car_city_act2all_staging",
          "vz_state_chicon002_HQ_Pristine",
          "vz_state_chicon002_Depot_Pristine",
          "vz_state_chicon002_Bridge_Pristine"
        })
        MrxLayerManager.MarkForAddition({
          "vz_state_car_city_act2chi",
          "vz_state_car_city_act2chi_staging",
          "vz_state_mar_city_act3",
          "vz_state_chicon002_HQ_Destroyed",
          "vz_state_chicon002_Depot_Destroyed",
          "vz_state_chicon002_Bridge_Destroyed"
        })
      end
    },
    ChiCon003 = {
      fPrereq = function()
        return HasKey("ChiCon003")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        
        local function c()
          WifVzBoundary.EnableExclusionBoundary("Boundary_CHHQ_Exclusion", true)
          WifVzBoundary.DrawExclusionBoundaryOnMap("Boundary_CHHQ_Exclusion", true)
          MrxTransit.SetLocationIsNuked(12, true)
          MrxTransit.SetLocationIsNuked(30, true)
          UnlockMission("PmcCon004")
          if not MrxCheatBootstrap.IsSkipModeEnabled() then
            MrxState.Exit(MrxState.STATE_WAITFORGAME, _EndBlockingSequence)
          else
            _EndBlockingSequence()
          end
        end
        
        local function b()
          local tStartLocations = GetMissionStartLocations("PmcCon004")
          if type(tStartLocations) == "table" then
            MrxUtil.TeleportHeroesToLocations(tStartLocations, c)
          end
        end
        
        local function a()
          WifHints.RemoveActiveHint("FionaHint01")
          WifHints.RemoveActiveHint("FionaHint02")
          WifHints.RemoveActiveHint("FionaHint03")
          WifHints.RemoveActiveHint("FionaHint04")
          WifHints.RemoveActiveHint("FionaHint05")
          WifHints.RemoveActiveHint("FionaHint07")
          WifHints.AddActiveHint("FionaHint06")
          MrxVerifyManager.SetKilledIfNotSet("ChiCon003_HVT")
          MrxLayerManager.MarkForRemoval({
            "vz_state_cumana_act1",
            "vz_state_car_city_act1",
            "vz_state_car_city_act2chi",
            "vz_state_car_city_act2chi_staging",
            "vz_state_car_city_act2all_staging",
            "vz_state_car_city_act2all",
            "vz_state_car_city_act3all"
          })
          MrxLayerManager.MarkForAddition({
            "vz_state_PmcCon004_ChinaNuked",
            "vz_state_car_city_act3chi"
          })
          MrxLayerManager.ProcessMarkedLayers()
          _PlayMovie({
            sMovie = "14_CVI_" .. sHeroLetter,
            fCallback = b,
            bSubtitles = true
          })
          MrxAchievements.NetGrantAchievement("ACHIEVEMENT_NO_COMPROMISE")
        end
        
        if not MrxCheatBootstrap.IsSkipModeEnabled() then
          MrxState.Enter(MrxState.STATE_WAITFORGAME, a)
        else
          a()
        end
      end
    },
    ChiCon051 = {
      fPrereq = function()
        return HasKey("ChiCon051")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        if not HasKey("PmcCon004") then
          UnlockMission("ChiCon008")
        end
        UnlockMission("ChiCon053")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    ChiCon053 = {
      fPrereq = function()
        return HasKey("ChiCon053")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        UnlockMission("ChiCon009")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    ChiOutposts = {
      fPrereq = function()
        return HasKey("ChiCon050") and HasKey("ChiCon051") and HasKey("ChiCon053")
      end,
      fConseq = function()
        MrxAchievements.NetGrantAchievement("ACHIEVEMENT_PIPELINE")
      end
    },
    ChiJob002 = {
      fPrereq = function()
        return HasKey("ChiJob002")
      end,
      fConseq = function()
        MrxVerifyManager.SetKilledIfNotSet("ChiJob002_Target_01")
        MrxVerifyManager.SetKilledIfNotSet("ChiJob002_Target_02")
        MrxVerifyManager.SetKilledIfNotSet("ChiJob002_Target_03")
        MrxVerifyManager.SetKilledIfNotSet("ChiJob002_Target_04")
        MrxVerifyManager.SetKilledIfNotSet("ChiJob002_Target_05")
        MrxVerifyManager.SetKilledIfNotSet("ChiJob010_Target_01")
        MrxVerifyManager.SetKilledIfNotSet("ChiJob010_Target_02")
        MrxVerifyManager.SetKilledIfNotSet("ChiJob010_Target_03")
        MrxVerifyManager.SetKilledIfNotSet("ChiJob010_Target_04")
        MrxVerifyManager.SetKilledIfNotSet("ChiJob010_Target_05")
      end
    },
    PirCon001 = {
      fPrereq = function()
        return HasKey("PirCon001")
      end,
      fConseq = function()
        _RemoveIntro("PmcBoss", "Pir")
        MrxUnlockFanfare.AddUnlockedItem({sType = "bounty", sFactionId = "Pir"})
        UnlockMission("PirCon002")
        UnlockMission("PirCon051")
        UnlockMission("PirJob012")
        UnlockMission("PirJob020")
      end
    },
    PirCon051 = {
      fPrereq = function()
        return HasKey("PirCon051")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        UnlockMission("PirCon003")
        UnlockMission("PirCon052")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    PirCon052 = {
      fPrereq = function()
        return HasKey("PirCon052")
      end,
      fConseq = function()
        _BeginBlockingSequence()
        UnlockMission("PirCon004")
        _ChangeOutpostStaging(_EndBlockingSequence)
      end
    },
    PrOutposts = {
      fPrereq = function()
        return HasKey("PirCon051") and HasKey("PirCon052")
      end,
      fConseq = function()
        MrxAchievements.NetGrantAchievement("ACHIEVEMENT_PIPELINE")
      end
    },
    PirJob012 = {
      fPrereq = function()
        return HasKey("PirJob012")
      end,
      fConseq = function()
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_01")
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_02")
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_03")
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_04")
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_05")
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_06")
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_07")
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_08")
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_09")
        MrxVerifyManager.SetKilledIfNotSet("PirJob012_Target_10")
      end
    }
  }
end

function Reset(bResetMore)
  MrxMissionFlow.Reset(bResetMore)
  SetFlowData(GetOriginalFlowData())
end

function _AddIntro(sStarterName, sIntroName)
  local oStarter = MrxStarterManager.GetStarter(sStarterName)
  if oStarter then
    oStarter:AddIntro(sIntroName)
    if MrxCheatBootstrap.IsSkipModeEnabled() then
      oStarter:SetViewedIntro(sIntroName, true)
    end
  end
end

function _RemoveIntro(sStarterName, sIntroName)
  local oStarter = MrxStarterManager.GetStarter(sStarterName)
  if oStarter then
    oStarter:RemoveIntro(sIntroName)
  end
end

function _PlayMovie(tArgs)
  if MrxCheatBootstrap.IsSkipModeEnabled() then
    MrxUtil.CallWithOptionalArgs(tArgs.fCallback, tArgs.tCallbackData)
  else
    Hud.Cinematic:Show({
      sMovie = tArgs.sMovie,
      fCallback = tArgs.fCallback,
      tCallbackData = tArgs.tCallbackData,
      bSubtitles = tArgs.bSubtitles
    })
  end
end

function _ChangeOutpostStaging(fCallback, tCallbackArgs)
  if not MrxCheatBootstrap.IsSkipModeEnabled() then
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
  else
    MrxLayerManager.ProcessMarkedLayers(MrxUtil.CallWithOptionalArgs, {fCallback, tCallbackArgs})
  end
end

function _AddHeroCostume()
  local n = WifPmcInterior.GetAvailableCostumes() or 1
  local tUnlockedCostumes = WifPmcInterior.SetAvailableCostumes(n + 1)
  if 1 <= #tUnlockedCostumes then
    for i, sPlayerVisibleName in ipairs(tUnlockedCostumes) do
      MrxUnlockFanfare.AddUnlockedItem({sType = "outfit", sName = sPlayerVisibleName})
    end
  end
end

NETEVENT_CLIENTCREDITS = 0

function NetEventCallback(nEventType)
  if nEventType == NETEVENT_CLIENTCREDITS then
    Event.Create(Event.GameStateChange, {"cinematic", "exit"}, _ClientStartCredits)
  end
end

function _ClientStartCredits()
  MrxGui.FadeToColor(0)
  Sys.RequestGameState("cinematic")
  local oCredits = MrxGui.FlashWidget:new()
  oCredits:SetFullscreen(true)
  oCredits:SetSwfFile("credits", _ClientEndCredits, {oCredits})
  MrxGui.AddWidget(oCredits)
end

function _ClientEndCredits(oCredits)
  oCredits:SetFlashEventHandler("creditsEnd", _ClientQuitToShell, {oCredits})
end

function _ClientQuitToShell(oCredits)
  oCredits:SetSwfFile(nil)
  MrxGui.RemoveWidget(oCredits)
  oCredits:delete()
  MrxGui.FadeFromColor()
  Sys.RequestGameState("unloading")
  Net.QuitGame()
end
