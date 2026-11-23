local L0_1, L1_1, L2_1, L3_1, L4_1, L5_1, L6_1, L7_1, L8_1, L9_1, L10_1, L11_1, L12_1, L13_1
import("MrxGui", false)
import("MrxCheatBootstrap", false)
import("MrxFactionManager", false)
import("MrxGuiBootstrap", false)
import("MrxLayerManager", false)
import("MrxMultiPageMenu", false)
import("MrxPlayer", false)
import("MrxPlayState", false)
import("MrxPmc", false)
import("MrxSound", false)
import("MrxStarterManager", false)
import("MrxState", false)
import("MrxUtil", false)
import("WifFreePlay", false)
import("WifMissionData", false)
import("WifMissionFlow", false)
import("WifPmcGarage", false)
import("WifVzBoundary", false)
import("MrxHq", false)
import("WifBriefingData", false)
import("MrxTransit", false)
import("MrxVoSequence", false)
import("MrxRewardData", false)
import("MrxGuiInterface", false)
L0_1 = {}
L1_1 = {}
L1_1.sSourceObject = "HqInterior"
L1_1.sBriefingLoc = "PmcInterior_StarterFiona_BriefingLoc"
L1_1.sHero1BriefingLoc = "PmcInterior_StarterFiona_BriefingLoc_Hero1"
L1_1.sWldBlpTexture = "HUD_PMC_Fiona"
L0_1.PmcBoss = L1_1
L1_1 = {}
L1_1.sSourceObject = "_pmcoutpost_interior_recruitheli 0x000c73ec"
L1_1.sLayerName = "Vz_State_PmcInterior_Hel"
L1_1.sBriefingLoc = "PmcInterior_StarterEwen_BriefingLoc"
L1_1.sHero1BriefingLoc = "PmcInterior_StarterEwen_BriefingLoc_Hero1"
L1_1.sWldBlpTexture = "HUD_PMC_Ewan"
L0_1.HelPmcBoss = L1_1
L1_1 = {}
L1_1.sSourceObject = "_pmcoutpost_interior_recruitmechanic 0x000c73ee"
L1_1.sLayerName = "Vz_State_PmcInterior_Mec"
L1_1.sAbsentLayerName = "Vz_State_PmcInterior_MecAbsent"
L1_1.sBriefingLoc = "PmcInterior_StarterEva_BriefingLoc"
L1_1.sHero1BriefingLoc = "PmcInterior_StarterEva_BriefingLoc_Hero1"
L1_1.sWldBlpTexture = "HUD_PMC_Eva"
L0_1.MecPmcBoss = L1_1
L1_1 = {}
L1_1.sSourceObject = "_pmcoutpost_interior_recruitjet 0x000c740d"
L1_1.sLayerName = "Vz_State_PmcInterior_Jet"
L1_1.sBriefingLoc = "PmcInterior_StarterMisha_BriefingLoc"
L1_1.sHero1BriefingLoc = "PmcInterior_StarterMisha_BriefingLoc_Hero1"
L1_1.sWldBlpTexture = "HUD_PMC_Misha"
L0_1.JetPmcBoss = L1_1
_tStarters = L0_1
_tPortals = {}
L0_1 = {}
L1_1 = {}
L1_1.sExterior_Entrance = "Starter_Pmc_Entrance"
L1_1.sExterior1 = "Starter_Pmc_Start1"
L1_1.sExterior2 = "Starter_Pmc_Start2"
L1_1.sInterior_Exit = "PmcInterior_A_Exit"
L1_1.sInterior1 = "PmcInterior_A1"
L1_1.sInterior2 = "PmcInterior_A2"
L1_1.sInteriorRoom = "MainHall"
L2_1 = {}
L2_1.sExterior_Entrance = "Pmc_B_Entrance"
L2_1.sExterior1 = "Pmc_B1"
L2_1.sExterior2 = "Pmc_B2"
L2_1.sInterior_Exit = "PmcInterior_B_Exit"
L2_1.sInterior1 = "PmcInterior_B1"
L2_1.sInterior2 = "PmcInterior_B2"
L2_1.sInteriorRoom = "MainHall"
L3_1 = {}
L3_1.sExterior_Entrance = "Pmc_C_Entrance"
L3_1.sExterior1 = "Pmc_C1"
L3_1.sExterior2 = "Pmc_C2"
L3_1.sInterior_Exit = "PmcInterior_C_Exit"
L3_1.sInterior1 = "PmcInterior_C1"
L3_1.sInterior2 = "PmcInterior_C2"
L3_1.sInteriorRoom = "MainHall"
L4_1 = {}
L4_1.sExterior_Entrance = "Pmc_D_Entrance"
L4_1.sExterior1 = "Pmc_D1"
L4_1.sExterior2 = "Pmc_D2"
L4_1.sInterior_Exit = "PmcInterior_D_Exit"
L4_1.sInterior1 = "PmcInterior_D1"
L4_1.sInterior2 = "PmcInterior_D2"
L4_1.sInteriorRoom = "MainHall"
L0_1[1] = L1_1
L0_1[2] = L2_1
L0_1[3] = L3_1
L0_1[4] = L4_1
_tPortalData = L0_1
_tInteriorPortalData = {}
L0_1 = {}
L1_1 = {}
L2_1 = {}
L2_1[1] = "player_chris_job_briefing_greeting"
L2_1[2] = "player_chris_job_briefing_idle"
L2_1[3] = "player_chris_job_briefing_no"
L2_1[4] = "player_chris_job_briefing_yes"
L2_1[5] = "player_chris_job_briefing_spiel"
L1_1.Chris = L2_1
L2_1 = {}
L2_1[1] = "player_jennifer_job_briefing_greeting"
L2_1[2] = "player_jennifer_job_briefing_idle"
L2_1[3] = "player_jennifer_job_briefing_no"
L2_1[4] = "player_jennifer_job_briefing_yes"
L2_1[5] = "player_jennifer_job_briefing_spiel"
L1_1.Jennifer = L2_1
L2_1 = {}
L2_1[1] = "player_mattias_job_briefing_greeting_fb"
L2_1[2] = "player_mattias_job_briefing_idle_fb"
L2_1[3] = "player_mattias_job_briefing_no_fb"
L2_1[4] = "player_mattias_job_briefing_yes_fb"
L2_1[5] = "player_mattias_job_briefing_spiel_fb"
L1_1.Mattias = L2_1
L2_1 = {}
L2_1[1] = "all_starter02_job_briefing_idle"
L2_1[2] = "all_starter02_job_briefing_greeting_neutral"
L2_1[3] = "all_starter02_job_briefing_greeting_happy"
L2_1[4] = "all_starter02_job_briefing_greeting_angry"
L2_1[5] = "all_starter02_job_briefing_spiel"
L2_1[6] = "all_starter02_job_briefing_goodbye"
L1_1.MaleStarter = L2_1
L2_1 = {}
L2_1[1] = "all_starter03_job_briefing_idle"
L2_1[2] = "all_starter03_job_briefing_greeting_neutral"
L2_1[3] = "all_starter03_job_briefing_greeting_happy"
L2_1[4] = "all_starter03_job_briefing_greeting_angry"
L2_1[5] = "all_starter03_job_briefing_spiel"
L2_1[6] = "all_starter03_job_briefing_goodbye"
L1_1.FemaleStarter = L2_1
L0_1.animation = L1_1
L1_1 = {}
L2_1 = {}
L2_1[1] = "Global_Job_Briefing_Chris"
L1_1.Chris = L2_1
L2_1 = {}
L2_1[1] = "Global_Job_Briefing_Jennifer"
L1_1.Jennifer = L2_1
L2_1 = {}
L2_1[1] = "Global_Job_Briefing_Mattias"
L1_1.Mattias = L2_1
L0_1.facefxanimationset = L1_1
L1_1 = {}
L1_1[1] = "vo_job_heros"
L1_1[2] = "vo_job_pmc"
L0_1.wavebank = L1_1
L1_1 = {}
L1_1[1] = "vo_job_heros"
L1_1[2] = "vo_job_pmc"
L0_1.soundbank = L1_1
_tAssetPreload = L0_1
L0_1 = {}
L1_1 = {}
L2_1 = {}
L2_1.Name = "Original"
L2_1.Model = "pmc_hum_chris"
L2_1.PlayerVisibleName = "[SHELL.Misc.41]"
L3_1 = {}
L3_1.Name = "Vacation"
L3_1.Model = "pmc_hum_chris_v2"
L3_1.PlayerVisibleName = "[SHELL.Misc.49]"
L4_1 = {}
L4_1.Name = "Commando"
L4_1.Model = "pmc_hum_chris_v3"
L4_1.PlayerVisibleName = "[SHELL.Misc.50]"
L5_1 = {}
L5_1.Name = "OffDuty"
L5_1.Model = "pmc_hum_chris_v4"
L5_1.PlayerVisibleName = "[SHELL.Misc.51]"
L6_1 = {}
L6_1.Name = "ChickenSuit"
L6_1.Model = "pmc_hum_chris_chickensuit"
L6_1.PlayerVisibleName = "[SHELL.Misc.45]"
L7_1 = {}
L7_1.Name = "PirateBoss"
L7_1.Model = "pr_hum_boss"
L7_1.PlayerVisibleName = "[human.pr.soldier]"
L7_1.bNoBriefing = true
L8_1 = {}
L8_1.Name = "Blanco"
L8_1.Model = "pmc_hum_blanco"
L8_1.PlayerVisibleName = "[SHELL.Bio.Name.Blanco]"
L8_1.bNoBriefing = true
L9_1 = {}
L9_1.Name = "Obama"
L9_1.Model = "pmc_hum_obama"
L9_1.PlayerVisibleName = "[SHELL.Costumes.obama]"
L9_1.bAlwaysVisible = true
L9_1.bNoBriefing = true
L10_1 = {}
L10_1.Name = "Palin"
L10_1.Model = "pmc_hum_sarah"
L10_1.PlayerVisibleName = "[SHELL.Costumes.palin]"
L10_1.bAlwaysVisible = true
L10_1.bNoBriefing = true
L1_1[1] = L2_1
L1_1[2] = L3_1
L1_1[3] = L4_1
L1_1[4] = L5_1
L1_1[5] = L6_1
L1_1[6] = L7_1
L1_1[7] = L8_1
L1_1[8] = L9_1
L1_1[9] = L10_1
L0_1.chris = L1_1
L1_1 = {}
L2_1 = {}
L2_1.Name = "Original"
L2_1.Model = "pmc_hum_jen"
L2_1.PlayerVisibleName = "[SHELL.Misc.41]"
L3_1 = {}
L3_1.Name = "Rebel"
L3_1.Model = "pmc_hum_jen_v3"
L3_1.PlayerVisibleName = "[SHELL.Misc.46]"
L4_1 = {}
L4_1.Name = "Tactical"
L4_1.Model = "pmc_hum_jen_v5"
L4_1.PlayerVisibleName = "[SHELL.Misc.54]"
L5_1 = {}
L5_1.Name = "NoJacket"
L5_1.Model = "pmc_hum_jen_v2"
L5_1.PlayerVisibleName = "[SHELL.Misc.47]"
L6_1 = {}
L6_1.Name = "CatSuit"
L6_1.Model = "pmc_hum_jen_v4"
L6_1.PlayerVisibleName = "[SHELL.Misc.48]"
L7_1 = {}
L7_1.Name = "ChickenSuit"
L7_1.Model = "pmc_hum_jen_chickensuit"
L7_1.PlayerVisibleName = "[SHELL.Misc.45]"
L8_1 = {}
L8_1.Name = "Fiona"
L8_1.Model = "pmc_hum_fiona_unlockable"
L8_1.PlayerVisibleName = "[human.pmc.fiona]"
L8_1.bNoBriefing = true
L9_1 = {}
L9_1.Name = "Eva"
L9_1.Model = "pmc_hum_mechanic"
L9_1.PlayerVisibleName = "[human.pmc.eva]"
L9_1.bNoBriefing = true
L10_1 = {}
L10_1.Name = "Obama"
L10_1.Model = "pmc_hum_obama"
L10_1.PlayerVisibleName = "[SHELL.Costumes.obama]"
L10_1.bAlwaysVisible = true
L10_1.bNoBriefing = true
L11_1 = {}
L11_1.Name = "Palin"
L11_1.Model = "pmc_hum_sarah"
L11_1.PlayerVisibleName = "[SHELL.Costumes.palin]"
L11_1.bAlwaysVisible = true
L11_1.bNoBriefing = true
L1_1[1] = L2_1
L1_1[2] = L3_1
L1_1[3] = L4_1
L1_1[4] = L5_1
L1_1[5] = L6_1
L1_1[6] = L7_1
L1_1[7] = L8_1
L1_1[8] = L9_1
L1_1[9] = L10_1
L1_1[10] = L11_1
L0_1.jennifer = L1_1
L1_1 = {}
L2_1 = {}
L2_1.Name = "Original"
L2_1.Model = "pmc_hum_mattias"
L2_1.PlayerVisibleName = "[SHELL.Misc.41]"
L3_1 = {}
L3_1.Name = "MetalHead"
L3_1.Model = "pmc_hum_mattias_v3"
L3_1.PlayerVisibleName = "[SHELL.Misc.43]"
L4_1 = {}
L4_1.Name = "Suit"
L4_1.Model = "pmc_hum_mattias_v2"
L4_1.PlayerVisibleName = "[SHELL.Misc.42]"
L5_1 = {}
L5_1.Name = "Jacket"
L5_1.Model = "pmc_hum_mattias_v4"
L5_1.PlayerVisibleName = "[SHELL.Misc.44]"
L6_1 = {}
L6_1.Name = "ChickenSuit"
L6_1.Model = "pmc_hum_mattias_chickensuit"
L6_1.PlayerVisibleName = "[SHELL.Misc.45]"
L7_1 = {}
L7_1.Name = "Ewan"
L7_1.Model = "pmc_hum_helipilot_unlockable"
L7_1.PlayerVisibleName = "[human.pmc.ewan]"
L7_1.bNoBriefing = true
L8_1 = {}
L8_1.Name = "Misha"
L8_1.Model = "pmc_hum_proppilot_unlockable"
L8_1.PlayerVisibleName = "[human.pmc.misha]"
L8_1.bNoBriefing = true
L9_1 = {}
L9_1.Name = "Obama"
L9_1.Model = "pmc_hum_obama"
L9_1.PlayerVisibleName = "[SHELL.Costumes.obama]"
L9_1.bAlwaysVisible = true
L9_1.bNoBriefing = true
L10_1 = {}
L10_1.Name = "Palin"
L10_1.Model = "pmc_hum_sarah"
L10_1.PlayerVisibleName = "[SHELL.Costumes.palin]"
L10_1.bAlwaysVisible = true
L10_1.bNoBriefing = true
L11_1 = {}
L11_1.Name = "GrandpaMattias"
L11_1.Model = "pmc_hum_mattias_v5"
L11_1.PlayerVisibleName = "[SHELL.Costumes.oldmatt]"
L11_1.bAlwaysVisible = true
L11_1.bNoBriefing = true
L1_1[1] = L2_1
L1_1[2] = L3_1
L1_1[3] = L4_1
L1_1[4] = L5_1
L1_1[5] = L6_1
L1_1[6] = L7_1
L1_1[7] = L8_1
L1_1[8] = L9_1
L1_1[9] = L10_1
L1_1[10] = L11_1
L0_1.mattias = L1_1
_tOutfits = L0_1
_tStockpileQty = {}
L0_1 = {}
L1_1 = {}
L1_1[1] = 1000
L1_1[2] = 976562
L1_1[3] = 1953125
L1_1[4] = 3906250
L1_1[5] = 7812500
L1_1[6] = 15625000
L1_1[7] = 31250000
L1_1[8] = 625000000
L1_1[9] = 125000000
L1_1[10] = 250000000
L1_1[11] = 500000000
L1_1[12] = 1000000000
L0_1.money = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 5
L1_1[3] = 9
L0_1.artillery = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 5
L1_1[3] = 9
L0_1.bombingrun = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 2
L1_1[3] = 4
L0_1.bunkerbuster = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 4
L1_1[3] = 7
L0_1.clusterbomb = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 3
L1_1[3] = 5
L0_1.combatairpatrol = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 2
L1_1[3] = 3
L0_1.fuelairbomb = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 4
L1_1[3] = 7
L0_1.laserguidedbomb = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 2
L1_1[3] = 2
L0_1.moab = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 2
L1_1[3] = 3
L0_1.rocketartillery = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 4
L1_1[3] = 7
L0_1.surgicalstrike = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 2
L1_1[3] = 4
L0_1.tankbuster = L1_1
L1_1 = {}
L1_1[1] = 1
L1_1[2] = 2
L1_1[3] = 4
L0_1.daisycutter = L1_1
_tStockpile = L0_1
L0_1 = {}
L0_1[1] = "_pmcoutpost_bld_hq_livedin 0x000d3c77"
L0_1[2] = "_pmcoutpost_bld_hqgarage_livedin 0x000d3c78"
L0_1[3] = "_pmcoutpost_bld_hqsuites 0x000cf8c2"
_tBuildings = L0_1
_tBuildingEvents = {}
_tBuildingStates = {}
_bTeleport = true

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  _bUnlocked = true
  RefreshUiDisplay()
  WifPmcGarage.Unlock()
  L0_2 = ipairs
  L1_2 = _tBuildings
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    _OnPmcHibernation(L3_2, true)
  end
end

Unlock = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _bUnlocked
  return L0_2
end

IsUnlocked = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _tBuildingStates[2]
  return L0_2
end

IsGarageAlive = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  _sWagerMissionId = A0_2
  _bWagerWin = A1_2
  _bWagerMissionComplete = true
  L3_2 = MrxRewardData.GetRewards(_sWagerMissionId).nWagered
  L4_2 = _bWagerWin
  if not L4_2 and L3_2 then
    L3_2 = -L3_2
  end
  if L3_2 ~= nil then
    MrxPmc.AddCashQty(L3_2, nil, "[Generic.Wagers]", true)
  end
end

SetWagerStatus = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _sWagerMissionId
  L1_2 = _bWagerWin
  return L0_2, L1_2
end

GetWagerStatus = L0_1

function L0_1(A0_2)
  local L1_2
  _bEntranceLock = A0_2
end

SetEntranceLock = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  L2_2 = Net.IsClient()
  if L2_2 then
    return
  end
  L2_2 = Net.IsServer()
  if L2_2 then
    Net.SetLoadingScreen(true)
  end
  _bEntering = true
  L2_2 = MrxCheatBootstrap.IsSkipModeEnabled()
  if not L2_2 then
    _bTeleport = A0_2
    L2_2 = _OnEnter
    L3_2 = A1_2 or L3_2
    if not A1_2 then
      L3_2 = 1
    end
    L2_2(L3_2)
  else
    MrxUtil.CallWithOptionalArgs(_fTeleportCallback, _tTeleportCallbackArgs)
    MrxUtil.CallWithOptionalArgs(_fUnloadCallback, _tUnloadCallbackArgs)
  end
end

Enter = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L3_2 = type(Human.Inventory.GetSecondaryWeapon(Player.GetLocalCharacter()))
  if L3_2 ~= "userdata" then
    L3_2 = Pg.GetGuidByName("Assault Rifle")
    L4_2 = Pg.GetGuidByName("C4")
    L5_2 = Pg.GetGuidByName("Grenade")
    L8_2 = {}
    L8_2[1] = L3_2
    L8_2[2] = L4_2
    L8_2[3] = L5_2
    Human.Inventory.SetAllWeapons(L1_2, L8_2)
    uNewC4 = Human.Inventory.GetSecondaryWeapon(L1_2)
    Weapon.SetReserveAmmo(uNewC4, 3)
    uNewRifle = Human.Inventory.GetPrimaryWeapon(L1_2)
    Weapon.SetReserveAmmo(uNewRifle, 6)
  end
  L3_2 = _bInside
  if L3_2 then
    return
  end
  _bInside = true
  _bEntering = true
  MrxFactionManager.DisableReporting(true)
  L3_2 = Net.IsServer()
  if L3_2 then
    L3_2 = Net.IsServer()
    if L3_2 then
      Net.SetLoadingScreen(true)
    end
    Net.SetBriefingInterior("WifPmcInterior")
    L3_2 = Event.CreatePersistent
    L4_2 = Event.ScriptEvent
    L5_2 = {}
    L6_2 = "mpPlayerJoin"
    
    function L7_2(A0_3)
      local L1_3
      L1_3 = true
      return L1_3
    end
    
    L5_2[1] = L6_2
    L5_2[2] = L7_2
    _evClientJoinedPMC = L3_2(L4_2, L5_2, _OnPlayerJoined)
  end
  L6_2 = {}
  L6_2[1] = A0_2
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _CompleteOnEnter, L6_2)
end

_OnEnter = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  MrxHq.GlobalEnter(true)
  _bHeroTeleportComplete = nil
  _bAssetsLoaded = nil
  _nPortal = A0_2
  _SetupStarters()
  _SetPmcTransitLocation(false)
  MrxPlayer.RiseFromYourGrave()
  WifFreePlay.StopNag()
  L1_2 = Sound.StopAndFlushAllSounds
  if L1_2 then
    Sound.StopAndFlushAllSounds()
  end
  _LoadInterior()
  dynamic_import("MrxBriefing", _BriefingModuleLoaded)
  RefreshUiDisplay()
  Graphics.Camera.SetNearFar(0, 0.3, 500, 0)
end

_CompleteOnEnter = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  _tLayers = _GetStarterLayers()
  MrxLayerManager.Add(_tLayers, _OnInteriorLoad, nil, nil, nil, true)
end

_LoadInterior = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L0_2 = {}
  L0_2[1] = "Vz_State_PmcInterior"
  L1_2 = pairs
  L2_2 = _tStarters
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = L5_2.oStarter
    if L6_2 then
      L6_2 = L5_2.sLayerName
      if L6_2 then
        table.insert(L0_2, L5_2.sLayerName)
      end
    else
      L6_2 = L5_2.sAbsentLayerName
      if L6_2 then
        table.insert(L0_2, L5_2.sAbsentLayerName)
      end
    end
  end
  return L0_2
end

_GetStarterLayers = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_interior"), "pmc")
  L0_2 = _tPortalData[_nPortal]
  L1_2 = _bTeleport
  if L1_2 then
    L2_2 = {}
    L2_2[1] = L0_2.sInterior1
    L2_2[2] = L0_2.sInterior2
    MrxUtil.TeleportHeroesToLocations(L2_2, _LoadStarters)
  else
    _bTeleport = true
    _LoadStarters()
  end
  _sCurrentRoom = _tPortalData[_nPortal].sInteriorRoom
  _EnableInteriorPortals(true)
  _nPortal = nil
  _InitOutfitChange()
end

_OnInteriorLoad = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = "Fiona-Briefing-Contract-Oil020-13"
  return L0_2
end

NetSafeGetSpecialCaseGreeting = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  _EnableStarters(true)
  _bPreOilCon020 = nil
  L0_2 = nil
  L1_2 = pairs
  L2_2 = _tStarters
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = L5_2.oStarter
    if L6_2 then
      L8_2 = L6_2.GetOfferedBriefings(L6_2).OilCon020
      if L8_2 then
        _bPreOilCon020 = true
        _SetStarterChatter(L4_2, false)
      end
      L8_2 = L7_2.OilCon020
      if L8_2 then
        L8_2 = L7_2.PmcCon031
        if L8_2 then
          L0_2 = "Fiona-Briefing-Contract-Oil020-13"
        end
      end
      L6_2.SetSpecialCaseGreeting(L6_2, L0_2)
    end
  end
  L1_2 = 0
  L2_2 = 0
  
  function L3_2()
    local L0_3, L1_3
    L0_3 = (L1_2 + 1)
    L1_3 = L2_2
    if L0_3 == L1_3 then
      _Kickoff(1)
    end
  end
  
  L4_2 = pairs
  L5_2 = _tStarters
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    L9_2 = L8_2.oStarter
    if L9_2 then
      L2_2 = L2_2 + 1
      L9_2 = L8_2.oStarter
      L9_2.Load(L9_2, L3_2)
    end
  end
  if L2_2 == 0 then
    _Kickoff(1)
  end
end

_LoadStarters = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  _mBriefingModule = A0_2
  _AssociateStartersToBriefingModule()
  L4_2 = {}
  L4_2[1] = 2
  _mBriefingModule.LoadTableOfAssets(_tAssetPreload, _Kickoff, L4_2)
end

_BriefingModuleLoaded = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L0_2 = pairs
  L1_2 = _tStarters
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    L5_2 = L4_2.oStarter
    if L5_2 then
      L5_2 = L4_2.oStarter
      L5_2.SetBriefingModule(L5_2, _mBriefingModule)
    end
  end
end

_AssociateStartersToBriefingModule = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  if A0_2 == 1 then
    _bHeroTeleportComplete = true
    MrxUtil.CallWithOptionalArgs(_fTeleportCallback, _tTeleportCallbackArgs)
    SetTeleportCallback(nil, nil)
  elseif A0_2 == 2 then
    _bAssetsLoaded = true
  end
  L1_2 = _bHeroTeleportComplete
  if L1_2 then
    L1_2 = _bAssetsLoaded
    if L1_2 then
      goto lbl_25
    end
  end
  do return end
  ::lbl_25::
  _bHeroTeleportComplete = nil
  _bAssetsLoaded = nil
  _EnablePortals(true, true)
  _UpdateStockpile(false)
  MrxUtil.CallWithOptionalArgs(_fLoadCallback, _tLoadCallbackArgs)
  SetLoadCallback(nil, nil)
  _bEntering = nil
  L1_2 = _bWagerMissionComplete
  if L1_2 then
    _StartStarter(WifMissionData.GetMissionStarter(_sWagerMissionId))
    _bWagerMissionComplete = nil
    _sWagerMissionId = nil
    _bWagerWin = nil
  else
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
  end
  _SetupPreOilCon020Nag()
  _SetFakePDA(true)
end

_Kickoff = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2
  if not A0_2 then
    L1_2 = Pda.Map
    L1_2.SetFakePlayerLocation(L1_2, {})
    L1_2 = Player.GetAllPlayers()
    L2_2 = ipairs
    L3_2 = L1_2
    L2_2, L3_2, L4_2 = L2_2(L3_2)
    for L5_2, L6_2 in L2_2, L3_2, L4_2 do
      Player.SetInPmc(L6_2, false)
    end
    return
  end
  L1_2 = Player.GetAllPlayers()
  L2_2 = Pg.GetGuidByName("_pmcoutpost_bld_hq_livedin 0x000d3c77")
  if L2_2 == nil then
    L2_2 = Pg.GetGuidByName("PMC_CentralBuilding")
  end
  L3_2 = Pg.GetGuidByName("HqInterior")
  L4_2 = 0
  L5_2 = 0
  L6_2 = 0
  L7_2 = 0
  L8_2 = 0
  L9_2 = 0
  if L2_2 then
    L10_2 = Object.GetPosition
    L11_2 = L2_2
    L10_2, L11_2, L12_2 = L10_2(L11_2)
    L6_2 = L12_2
    L5_2 = L11_2
    L4_2 = L10_2
  end
  if L3_2 then
    L10_2 = Object.GetPosition
    L11_2 = L3_2
    L10_2, L11_2, L12_2 = L10_2(L11_2)
    L9_2 = L12_2
    L8_2 = L11_2
    L7_2 = L10_2
  end
  L10_2 = ipairs
  L11_2 = L1_2
  L10_2, L11_2, L12_2 = L10_2(L11_2)
  for L13_2, L14_2 in L10_2, L11_2, L12_2 do
    L15_2 = Player.GetCharacter(L14_2)
    if L15_2 then
      L16_2 = Object.GetPosition
      L17_2 = L15_2
      L16_2, L17_2, L18_2 = L16_2(L17_2)
      if L16_2 and L17_2 and L18_2 then
        L16_2 = L4_2 + (L7_2 - L16_2)
        L17_2 = L5_2 + (L8_2 - L17_2)
        L18_2 = L6_2 + (L9_2 - L18_2)
        L19_2 = Pda.Map
        L21_2 = {}
        L21_2.vPlayer = L14_2
        L21_2.nX = L16_2
        L21_2.nY = L17_2
        L21_2.nZ = L18_2
        L19_2.SetFakePlayerLocation(L19_2, L21_2)
      end
    end
    Player.SetInPmc(L14_2, true)
  end
end

_SetFakePDA = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L2_2 = _bInside
  if not L2_2 then
    return
  end
  _EnableInteriorPortals(false)
  _bInside = false
  L2_2 = Net.IsServer()
  if L2_2 then
    Net.SetLoadingScreen(true)
    Event.Delete(_evClientJoinedPMC)
    Event.Delete(_evClientQuitPMC)
    L2_2 = _evRemotePlayerAwake
    if L2_2 then
      Event.Delete(_evRemotePlayerAwake)
      _evRemotePlayerAwake = nil
    end
  end
  L5_2 = {}
  L5_2[1] = A0_2
  L5_2[2] = A1_2
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _OnExit, L5_2)
end

Exit = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L2_2 = _bExiting
  if L2_2 then
    return
  end
  _bExiting = true
  _nPortal = A0_2
  _sCurrentRoom = nil
  _EnablePortals(false, true)
  _EnableStarters(false)
  _SetPmcTransitLocation(true)
  _bWelcomedPlayer = nil
  L5_2 = {}
  L5_2[1] = 1
  L5_2[2] = A1_2
  MrxLayerManager.Remove(_tLayers, _ExitEnd, L5_2, true)
  if not A1_2 then
    L2_2 = _nPortal
    if L2_2 < 0 then
      _ExitEnd(2, false)
    else
      L2_2 = nil
      L3_2 = pairs
      L4_2 = _tStarters
      L3_2, L4_2, L5_2 = L3_2(L4_2)
      for L6_2, L7_2 in L3_2, L4_2, L5_2 do
        L8_2 = L7_2.oStarter
        if L8_2 then
          L2_2 = L8_2.GetPendingContract(L8_2)
          if L2_2 then
            break
          end
        end
      end
      L3_2 = nil
      if L2_2 then
        L3_2 = WifMissionFlow.GetMissionStartLocations(L2_2)
        _DoParkingLot(L2_2)
      else
        L4_2 = _tPortalData[_nPortal]
        L5_2 = {}
        L5_2[1] = L4_2.sExterior1
        L5_2[2] = L4_2.sExterior2
        L3_2 = L5_2
      end
      L7_2 = {}
      L7_2[1] = 2
      L7_2[2] = A1_2
      MrxUtil.TeleportHeroesToLocations(L3_2, _ExitEnd, L7_2)
    end
  end
  _nPortal = nil
  _DeinitOutfitChange()
  _tViewedIntros = _mBriefingModule.GetViewedIntros()
  L5_2 = {}
  L5_2[1] = 3
  L5_2[2] = A1_2
  _mBriefingModule.UnloadTableOfAssets(_tAssetPreload, _ExitEnd, L5_2)
  _mBriefingModule = nil
  L2_2 = Net.IsServer()
  if L2_2 then
    Net.SetBriefingInterior()
  end
  dynamic_remove("MrxBriefing")
  WifPmcGarage.CheckFionaCar(true)
  _StopPreOilCon020Nag()
end

_OnExit = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  if A0_2 == 1 then
    _bLayersRemoved = true
  elseif A0_2 == 2 then
    _bHeroTeleportComplete = true
  elseif A0_2 == 3 then
    _bAssetsUnloaded = true
  end
  L2_2 = _bLayersRemoved
  if L2_2 then
    if not A1_2 then
      L2_2 = _bHeroTeleportComplete
      if not L2_2 then
        goto lbl_26
      end
    end
    L2_2 = _bAssetsUnloaded
    if L2_2 then
      goto lbl_27
    end
  end
  ::lbl_26::
  do return end
  ::lbl_27::
  _bLayersRemoved = nil
  _bHeroTeleportComplete = nil
  _bAssetsUnloaded = nil
  L2_2 = Event.Create
  L3_2 = Event.TimerRelative
  L4_2 = {}
  L4_2[1] = 2
  
  function L5_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3, L11_3, L12_3
    RefreshUiDisplay()
    WifFreePlay.StartNag()
    L0_3 = A1_2
    if not L0_3 then
      L0_3 = {}
      L1_3 = nil
      L2_3 = pairs
      L3_3 = _tStarters
      L2_3, L3_3, L4_3 = L2_3(L3_3)
      for L5_3, L6_3 in L2_3, L3_3, L4_3 do
        L7_3 = L6_3.oStarter
        if L7_3 then
          L9_3 = L7_3
          L8_3 = L7_3.GetMissionsToBeAccepted
          L8_3, L9_3 = L8_3(L9_3)
          L0_3 = MrxUtil.MergeIndexedTables(L0_3, L8_3)
          L7_3.ResetIntraSessionData(L7_3)
          L7_3.Unload(L7_3)
        end
      end
      L2_3 = table.getn(L0_3)
      if 0 < L2_3 then
        WifMissionFlow.AcceptMissions(L0_3, L1_3)
      end
    end
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
    MrxUtil.CallWithOptionalArgs(_fUnloadCallback, _tUnloadCallbackArgs)
    SetUnloadCallback(nil, nil)
  end
  
  L2_2(L3_2, L4_2, L5_2)
  _SetFakePDA(false)
  MrxHq.GlobalExit()
  MrxState.Exit(MrxState.STATE_WAITFORGAME, _ExitComplete)
  _bExiting = nil
end

_ExitEnd = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L0_2 = _tViewedIntros
  if L0_2 then
    L0_2 = pairs
    L1_2 = _tViewedIntros
    L0_2, L1_2, L2_2 = L0_2(L1_2)
    for L3_2, L4_2 in L0_2, L1_2, L2_2 do
      L5_2 = WifBriefingData.Intros[L3_2].tHq
      L6_2 = ipairs
      L7_2 = L5_2
      L6_2, L7_2, L8_2 = L6_2(L7_2)
      for L9_2, L10_2 in L6_2, L7_2, L8_2 do
        L11_2 = L10_2
        L12_2 = "::Portal"
        L11_2 = L11_2 .. L12_2
        L12_2 = Hud.Radar
        L14_2 = {}
        L14_2.sName = L11_2
        L12_2.AnimateObjectiveSize(L12_2, L14_2)
      end
    end
    _tViewedIntros = nil
  end
end

_ExitComplete = L0_1

function L0_1()
  local L0_2, L1_2
  RemovePmcPdaBlip()
end

NetSafeRemovePmcPdaBlip = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  L0_2 = Pda.Map
  L2_2 = {}
  L2_2.sName = "Pmc"
  L2_2.bDontNetSync = true
  L0_2.RemoveBlip(L0_2, L2_2)
end

RemovePmcPdaBlip = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  AddPmcPdaBlip(A0_2, A1_2)
end

NetSafeAddPmcPdaBlip = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L1_2 = ""
  L2_2 = pairs
  L3_2 = A0_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = WifMissionData.GetMissionTitle(L6_2)
    L8_2 = WifMissionData.GetMissionFaction(L6_2)
    L9_2 = ""
    if L8_2 then
      L9_2 = MrxFactionManager.GetInlineIcon(L8_2) .. " "
    end
    L1_2 = L1_2 .. L9_2 .. "\"" .. L7_2 .. "\""
    L10_2 = WifMissionData.GetMissionRepeatable(L6_2)
    if L10_2 then
      L11_2 = WifMissionFlow.GetKeyValue(L6_2) + 1
      L12_2 = WifMissionData.GetMissionLevels(L6_2)
      if L11_2 > L12_2 then
        L11_2 = L12_2
      end
      L1_2 = L1_2 .. " ([Generic.Level] " .. L11_2 .. ")"
    end
    L1_2 = L1_2 .. "\n"
  end
  return L1_2
end

GetMissionDesc = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L2_2 = Pg.GetGuidByName("Starter_Pmc_Start1")
  L3_2 = 0
  L4_2 = ""
  L5_2 = _tStarters.MecPmcBoss.oStarter
  if L5_2 then
    L4_2 = L4_2 .. "[cash] [Briefing.Shop]\n"
  end
  L5_2 = _tStarters.HelPmcBoss.oStarter
  if L5_2 then
    L4_2 = L4_2 .. "[vehheli] [Briefing.Transit]\n"
  end
  if L4_2 ~= "" then
    L4_2 = L4_2 .. "\n"
  end
  L4_2 = L4_2 .. "[PDA.Map.WorkAvailableHeader]\n"
  L5_2 = {}
  L6_2 = {}
  L7_2 = pairs
  L8_2 = A1_2
  L7_2, L8_2, L9_2 = L7_2(L8_2)
  for L10_2, L11_2 in L7_2, L8_2, L9_2 do
    L13_2 = WifMissionData.IsMissionOnCriticalPath(WifMissionData.GetMissionIdFromIndex(L11_2))
    if L13_2 then
      table.insert(L5_2, L12_2)
    else
      table.insert(L6_2, L12_2)
    end
    L3_2 = L3_2 + 1
  end
  table.sort(L5_2)
  table.sort(L6_2)
  if L3_2 <= 0 then
    L4_2 = L4_2 .. "([Generic.None])"
  else
    L7_2 = table.getn(L5_2)
    if 0 < L7_2 then
      L4_2 = L4_2 .. GetMissionDesc(L5_2)
    end
    L7_2 = table.getn(L6_2)
    if 0 < L7_2 then
      L4_2 = L4_2 .. GetMissionDesc(L6_2)
    end
  end
  L7_2 = Pda.Map
  L9_2 = {}
  L9_2.sName = "Pmc"
  L7_2.RemoveBlip(L7_2, L9_2)
  L7_2 = MrxFactionManager.GetPdaFactionIdFromFactionId("Pmc")
  L8_2 = Pda.Map
  L10_2 = {}
  L10_2.sName = "Pmc"
  L10_2.uGuid = L2_2
  L10_2.sLabel = "[PDA.Map.Locations.PMC]"
  L10_2.sDesc = L4_2
  L10_2.sTexture = "icon_pmc_mc"
  L10_2.bSticky = A0_2
  L10_2.sMission = nil
  L10_2.sFaction = L7_2
  L10_2.bTodoList = false
  L10_2.bDontNetSync = true
  L8_2.AddBlip(L8_2, L10_2)
end

AddPmcPdaBlip = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  L0_2 = Net.IsClient()
  if L0_2 then
    return
  end
  L0_2 = true
  L1_2 = false
  L2_2 = MrxPlayState.IsFree()
  L3_2 = _bEntranceLock
  if L3_2 then
    L0_2 = false
  end
  L3_2 = pairs
  L4_2 = _tStarters
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    L8_2 = MrxStarterManager.GetStarter(L6_2)
    if L8_2 then
      L9_2 = false
      L10_2 = L8_2.GetIntros(L8_2)
      if L10_2 then
        L11_2 = pairs
        L12_2 = L10_2
        L11_2, L12_2, L13_2 = L11_2(L12_2)
        for L14_2, L15_2 in L11_2, L12_2, L13_2 do
          L16_2 = L8_2.HasViewedIntro(L8_2, L14_2)
          if not L16_2 then
            L9_2 = true
            break
          end
        end
      end
      L11_2 = L8_2.HasCriticalPathBriefings(L8_2)
      if L11_2 or L9_2 then
        L1_2 = true
      end
    end
  end
  if L1_2 then
    L1_2 = L2_2
  end
  if L0_2 then
    L3_2 = {}
    L4_2 = 0
    L5_2 = pairs
    L6_2 = _tStarters
    L5_2, L6_2, L7_2 = L5_2(L6_2)
    for L8_2, L9_2 in L5_2, L6_2, L7_2 do
      L10_2 = MrxStarterManager.GetStarter(L8_2)
      if L10_2 then
        L11_2 = L10_2.GetOfferedBriefings(L10_2)
        L12_2 = pairs
        L13_2 = L11_2
        L12_2, L13_2, L14_2 = L12_2(L13_2)
        for L15_2, L16_2 in L12_2, L13_2, L14_2 do
          L3_2[(L4_2 + 1)] = WifMissionData.GetMissionIndexFromId(L15_2)
        end
      end
    end
    L5_2 = 0
    if L1_2 ~= nil then
      if L1_2 == true then
        L5_2 = 1
      else
        L5_2 = 2
      end
    end
    AddPmcPdaBlip(L1_2, L3_2)
    Net.SendEvent_AddPmcPdaBlip(L5_2, L3_2)
  end
  if not L0_2 then
    L3_2 = _bDisplayed
    if not L3_2 then
      goto lbl_116
    end
  end
  if L0_2 then
    L3_2 = _bDisplayed
    if L3_2 then
      L3_2 = _bSticky
      ::lbl_116::
      if L1_2 == L3_2 then
        return
      end
    end
  end
  _bDisplayed = L0_2
  _bSticky = L1_2
  _EnablePortals(L0_2, false)
  L3_2 = Hud.Radar
  L5_2 = {}
  L5_2.sName = "Pmc"
  L3_2.RemoveObjective(L3_2, L5_2)
  if L0_2 then
    L3_2 = Pg.GetGuidByName("Starter_Pmc_Start1")
    L4_2 = Hud.Radar
    L6_2 = {}
    L6_2.sName = "Pmc"
    L6_2.uGuid = L3_2
    L6_2.nR = 255
    L6_2.nG = 255
    L6_2.nB = 255
    L6_2.nWidth = 8
    L6_2.nHeight = 8
    L6_2.sTexture = "MiniMap_Icon_Faction_PMC"
    L6_2.bSticky = L1_2
    L4_2.AddObjective(L4_2, L6_2)
  else
    RemovePmcPdaBlip()
    Net.SendEvent_RemovePmcPdaBlip()
  end
end

RefreshUiDisplay = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  L2_2 = _tPortals[A0_2]
  if A1_2 then
    L3_2 = "HUD_HQ_PMC"
    L4_2 = L2_2.bIsAnExit
    if L4_2 then
      L3_2 = "HUD_exit"
    end
    L2_2.uMarker = Marker.AddBlip(A0_2, L3_2, 32, 255, 255, 255, 255, 1.25, 20, 30)
    L4_2 = Net.IsServer()
    if L4_2 then
      Net.SendEvent_AddMarkerObjective(A0_2, L2_2.uMarker, 255, 255, 255, 1.25, MrxUtil.MarkerGetIndexByName_World(L3_2), 1, 16, false, 20, 30)
    end
    L4_2 = MrxUtil.GetPrimaryObjectiveRgb
    L4_2, L5_2, L6_2 = L4_2()
    L2_2.uDisc = Marker.AddDisc(A0_2, 0.5, L4_2, L5_2, L6_2, 0.1)
    L7_2 = Net.IsServer()
    if L7_2 then
      Net.SendEvent_AddMarkerObjective(A0_2, L2_2.uDisc, L4_2, L5_2, L6_2, 0.1, 0, 0.5, 0, true)
    end
  else
    L3_2 = L2_2.uMarker
    if L3_2 then
      Marker.Remove(L2_2.uMarker)
      L3_2 = Net.IsServer()
      if L3_2 then
        Net.SendEvent_RemoveMarkerObjective(L2_2.uMarker)
      end
      L2_2.uMarker = nil
    end
    L3_2 = L2_2.uDisc
    if L3_2 then
      Marker.Remove(L2_2.uDisc)
    end
    L3_2 = Net.IsServer()
    if L3_2 then
      Net.SendEvent_RemoveMarkerObjective(L2_2.uDisc)
    end
  end
end

_SetPortalMarker = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L4_2 = _tPortals[A0_2]
  if L4_2 then
    return
  end
  L5_2 = _tPortals
  L5_2[A0_2] = {}
  L4_2 = _tPortals[A0_2]
  L4_2.bIsAnExit = A1_2
  
  function L5_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3
    L0_3 = L4_2.uAwakeEvent
    if L0_3 then
      Event.Delete(L4_2.uAwakeEvent)
      L0_3 = L4_2
      L0_3.uAwakeEvent = nil
    end
    L0_3 = nil
    L1_3 = A1_2
    if L1_3 then
      L0_3 = "[ContextAction.Exit]"
    else
      L0_3 = "[ContextAction.Enter]"
    end
    Pg.AddContextAction(A0_2, L0_3, 2, 0, 0, 255, 2, false)
    L1_3 = L4_2
    L2_3 = Event.CreatePersistent
    L3_3 = Event.ContextAction
    L4_3 = {}
    L4_3[1] = 0
    L4_3[2] = A0_2
    L5_3 = A2_2
    L6_3 = A3_2
    if not L6_3 then
      L6_3 = {}
    end
    L1_3.uEvent = L2_3(L3_3, L4_3, L5_3, L6_3)
    _SetPortalMarker(A0_2, true)
  end
  
  L6_2 = Object.IsAwake(A0_2)
  if L6_2 then
    L5_2()
  else
    L8_2 = {}
    L8_2[1] = A0_2
    L8_2[2] = "awake"
    L4_2.uAwakeEvent = Event.Create(Event.ObjectHibernation, L8_2, L5_2)
  end
end

_AddPortal = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = _tPortals[A0_2]
  if not L1_2 then
    return
  end
  Pg.RemoveContextAction(A0_2)
  L2_2 = L1_2.uEvent
  if L2_2 then
    Event.Delete(L1_2.uEvent)
    L1_2.uEvent = nil
  end
  L2_2 = L1_2.uAwakeEvent
  if L2_2 then
    Event.Delete(L1_2.uAwakeEvent)
    L1_2.uAwakeEvent = nil
  end
  _SetPortalMarker(A0_2, false)
  L2_2 = _tPortals
  L2_2[A0_2] = nil
end

_RemovePortal = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L4_2 = MrxUtil.SpawnActor
  L5_2 = "location"
  L6_2 = (A0_2 .. "_portal")
  L7_2 = "HqInterior"
  L8_2 = A0_2
  L9_2 = 0
  L10_2 = false
  L11_2 = false
  
  function L12_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3
    _AddPortal(Pg.GetGuidByName(L3_2), false, A1_2, A2_2)
  end
  
  L4_2 = L4_2(L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2)
  return L4_2
end

_AddPortalAtHardpoint = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L2_2 = Pg.GetGuidByName((A0_2 .. "_portal"))
  _RemovePortal(L2_2)
  Object.Remove(L2_2)
end

_RemovePortalAtHardpoint = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L2_2 = ipairs
  L3_2 = _tPortalData
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = nil
    L8_2 = nil
    L9_2 = nil
    if A1_2 then
      L7_2 = Pg.GetGuidByName(L6_2.sInterior_Exit)
      L8_2 = Exit
      L10_2 = {}
      L10_2[1] = L5_2
      L10_2[2] = false
      L9_2 = L10_2
    else
      L7_2 = Pg.GetGuidByName(L6_2.sExterior_Entrance)
      L8_2 = _OnEnter
      L10_2 = {}
      L10_2[1] = L5_2
      L9_2 = L10_2
    end
    if A0_2 then
      _AddPortal(L7_2, A1_2, L8_2, L9_2)
    else
      _RemovePortal(L7_2)
    end
  end
end

_EnablePortals = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L1_2 = _sCurrentRoom
  if not L1_2 then
    return
  end
  L1_2 = ipairs
  L2_2 = _tInteriorPortalData
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = nil
    L7_2 = L5_2.sOriginRoom
    L8_2 = _sCurrentRoom
    if L7_2 == L8_2 then
      L6_2 = Pg.GetGuidByName(L5_2.sOrigin1)
    else
      L7_2 = L5_2.sTerminusRoom
      L8_2 = _sCurrentRoom
      if L7_2 == L8_2 then
        L6_2 = Pg.GetGuidByName(L5_2.sTerminus1)
      end
    end
    if L6_2 then
      if A0_2 then
        L11_2 = {}
        L11_2[1] = L4_2
        _AddPortal(L6_2, false, _OnInteriorPortalEnter, L11_2)
      else
        _RemovePortal(L6_2)
      end
    end
  end
end

_EnableInteriorPortals = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  _EnableInteriorPortals(false)
  L1_2 = _tInteriorPortalData[A0_2]
  L2_2 = nil
  L3_2 = nil
  L4_2 = nil
  L5_2 = L1_2.sOriginRoom
  L6_2 = _sCurrentRoom
  if L5_2 == L6_2 then
    L2_2 = true
    L3_2 = L1_2.sTerminus1
    L4_2 = L1_2.sTerminus2
    _sCurrentRoom = L1_2.sTerminusRoom
  else
    L2_2 = false
    L3_2 = L1_2.sOrigin1
    L4_2 = L1_2.sOrigin2
    _sCurrentRoom = L1_2.sOriginRoom
  end
  _EnableInteriorPortals(true)
  L6_2 = {}
  L6_2[1] = L3_2
  L6_2[2] = L4_2
  MrxUtil.TeleportHeroesToLocations(L6_2)
end

_OnInteriorPortalEnter = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = pairs
  L1_2 = _tStarters
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    L4_2.oStarter = MrxStarterManager.GetStarter(L3_2)
  end
end

_SetupStarters = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = pairs
  L2_2 = _tStarters
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    if A0_2 then
      _EnableStarter(L4_2)
    else
      _DisableStarter(L4_2)
    end
  end
end

_EnableStarters = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  if A1_2 then
    Pg.AddContextAction(A0_2, "[ContextAction.Talk]", 4, 0, 0, 255, 4, false)
  else
    Pg.RemoveContextAction(A0_2)
  end
end

_SetStarterContextAction = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2
  L2_2 = _tStarters[A0_2]
  if not L2_2 then
    return
  end
  L3_2 = L2_2.oStarter
  L4_2 = L2_2.uStarter
  if not L3_2 or not L4_2 then
    return
  end
  if A1_2 then
    L5_2 = false
    L6_2 = L3_2.GetOfferedBriefings(L3_2)
    if L6_2 then
      L7_2 = pairs
      L8_2 = L6_2
      L7_2, L8_2, L9_2 = L7_2(L8_2)
      for L10_2, L11_2 in L7_2, L8_2, L9_2 do
        L12_2 = WifMissionData.IsMissionOnCriticalPath(L10_2)
        if L12_2 then
          L5_2 = true
          break
        end
      end
    end
    L7_2 = L3_2.GetIntros(L3_2)
    if L7_2 then
      L8_2 = pairs
      L9_2 = L7_2
      L8_2, L9_2, L10_2 = L8_2(L9_2)
      for L11_2, L12_2 in L8_2, L9_2, L10_2 do
        L13_2 = L3_2.HasViewedIntro(L3_2, L11_2)
        if not L13_2 then
          L5_2 = true
          break
        end
      end
    end
    L8_2 = nil
    L9_2 = nil
    L10_2 = nil
    L11_2 = nil
    L12_2 = nil
    if L5_2 then
      L13_2 = MrxUtil.GetPrimaryObjectiveRgb
      L13_2, L14_2, L15_2 = L13_2()
      L10_2 = L15_2
      L9_2 = L14_2
      L8_2 = L13_2
      L11_2 = nil
      L12_2 = nil
    else
      L13_2 = MrxUtil.GetSecondaryObjectiveRgb
      L13_2, L14_2, L15_2 = L13_2()
      L10_2 = L15_2
      L9_2 = L14_2
      L8_2 = L13_2
      L11_2 = 20
      L12_2 = 30
    end
    L2_2._uMarker = Marker.AddBlip(L4_2, L2_2.sWldBlpTexture, 32, L8_2, L9_2, L10_2, 255, 2, L11_2, L12_2)
    L13_2 = Net.IsServer()
    if L13_2 then
      Net.SendEvent_AddMarkerObjective(L4_2, L2_2._uMarker, L8_2, L9_2, L10_2, 2, MrxUtil.MarkerGetIndexByName_World(L2_2.sWldBlpTexture), 1, 16, false, L11_2, L12_2)
    end
  else
    L5_2 = L2_2._uMarker
    if L5_2 then
      Marker.Remove(L2_2._uMarker)
      L5_2 = Net.IsServer()
      if L5_2 then
        Net.SendEvent_RemoveMarkerObjective(L2_2._uMarker)
      end
      L2_2._uMarker = nil
    end
  end
end

_SetStarterMarker = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L2_2 = _tStarters[A0_2]
  if not L2_2 then
    return
  end
  L3_2 = L2_2.oStarter
  L4_2 = L2_2.uStarter
  if not L3_2 or not L4_2 then
    return
  end
  if A1_2 then
    L7_2 = {}
    L7_2[1] = Player.GetAnyCharacter()
    L7_2[2] = L4_2
    L7_2[3] = "<"
    L7_2[4] = 7
    L9_2 = {}
    L9_2[1] = A0_2
    L2_2._uChatterEvent = Event.Create(Event.ObjectProximity, L7_2, _StarterChatter, L9_2)
    L2_2._bChatterEnabled = true
  else
    Event.Delete(L2_2._uChatterEvent)
    L2_2._uChatterEvent = nil
    L2_2._bChatterEnabled = nil
    L5_2 = _sCurrentStarterChatter
    if L5_2 == A0_2 then
      _sCurrentStarterChatter = nil
      MrxVoSequence.Stop(false, true, MrxVoSequence.knPriorityFreeplay)
    end
  end
end

_SetStarterChatter = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = _tStarters[A0_2].uStarter
  
  function L3_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3
    L2_3 = _tStarters[A0_3]._bChatterEnabled
    if not L2_3 then
      return
    end
    L4_3 = {}
    L4_3[1] = 20
    L6_3 = {}
    L6_3[1] = A0_3
    L6_3[2] = true
    L1_3._uChatterEvent = Event.Create(Event.TimerRelative, L4_3, _SetStarterChatter, L6_3)
    L2_3 = _sCurrentStarterChatter
    if L2_3 == A0_3 then
      _sCurrentStarterChatter = nil
    end
  end
  
  _ChatterComplete = L3_2
  L3_2 = _sCurrentStarterChatter
  if L3_2 then
    _ChatterComplete(A0_2)
  else
    L3_2 = _GetStarterChatterVo(A0_2)
    L4_2 = {}
    L5_2 = {}
    L5_2[1] = L3_2
    L5_2[2] = L2_2
    L6_2 = {}
    L8_2 = {}
    L8_2[1] = A0_2
    L6_2[1] = _ChatterComplete
    L6_2[2] = L8_2
    L4_2[1] = L5_2
    L4_2[2] = L6_2
    MrxVoSequence.Start(L4_2, false, MrxVoSequence.knPriorityFreeplay)
    Human.DoAction(L2_2, "Proximity")
    _sCurrentStarterChatter = A0_2
  end
end

_StarterChatter = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  L2_2 = _tStarters[A0_2].bEnabled
  if L2_2 then
    return
  end
  L2_2 = L1_2.oStarter
  L3_2 = L1_2.sSourceObject
  if not L2_2 or not L3_2 then
    return
  end
  L6_2 = Vehicle.GetRiders(Pg.GetGuidByName(L3_2))[1]
  L1_2.uStarter = L6_2
  L2_2.SetActor(L2_2, L6_2)
  _SetStarterContextAction(L6_2, true)
  L9_2 = {}
  L9_2[1] = 0
  L9_2[2] = L6_2
  L11_2 = {}
  L14_2 = {}
  L14_2[1] = A0_2
  L11_2[1] = MrxState.STATE_WAITFORGAME
  L11_2[2] = _StartStarter
  L11_2[3] = L14_2
  L1_2.uEvent = Event.CreatePersistent(Event.ContextAction, L9_2, MrxState.Enter, L11_2)
  _SetStarterMarker(A0_2, true)
  _SetStarterChatter(A0_2, true)
  L1_2.bEnabled = true
end

_EnableStarter = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  L1_2 = _tStarters[A0_2]
  if L1_2 then
    L2_2 = L1_2.bEnabled
    if L2_2 then
      goto lbl_9
    end
  end
  do return end
  ::lbl_9::
  L2_2 = L1_2.oStarter
  L2_2.SetActor(L2_2, nil)
  _SetStarterContextAction(L1_2.uStarter, false)
  L3_2 = L1_2.uEvent
  if L3_2 then
    Event.Delete(L1_2.uEvent)
    L1_2.uEvent = nil
  end
  _SetStarterMarker(A0_2, false)
  _SetStarterChatter(A0_2, false)
  L1_2.bEnabled = nil
end

_DisableStarter = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L1_2 = _tStarters[A0_2]
  L2_2 = L1_2.oStarter
  L4_2 = Pg.GetGuidByName(L1_2.sSourceObject)
  L6_2 = Vehicle.GetRiders(L4_2)[1]
  L1_2.uStarter = L6_2
  _sCurrentStarterId = A0_2
  Vehicle.Exit(L4_2, L6_2)
  _SetStarterContextAction(L6_2, false)
  _SetAllMarkers(false)
  _SetAllStarterChatter(false)
  L2_2.Start(L2_2)
  _StopPreOilCon020Nag()
end

_StartStarter = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L1_2 = _sCurrentStarterId
  if not L1_2 then
    return
  end
  L1_2 = _tStarters[_sCurrentStarterId]
  _sCurrentStarterId = nil
  if not A0_2 then
    L2_2 = L1_2.uStarter
    if L2_2 then
      Vehicle.Enter(Pg.GetGuidByName(L1_2.sSourceObject), L1_2.uStarter)
      _SetStarterContextAction(L1_2.uStarter, true)
      _SetAllMarkers(true)
      L4_2 = _bPreOilCon020
      if L4_2 then
        _SetupPreOilCon020Nag()
      else
        L6_2 = {}
        L6_2[1] = 2
        L8_2 = {}
        L8_2[1] = true
        Event.Create(Event.TimerRelative, L6_2, _SetAllStarterChatter, L8_2)
      end
    end
  end
end

BriefingComplete = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _bInside
  return L0_2
end

IsInside = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _bEntering
  return L0_2
end

IsEntering = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L0_2 = false
  L1_2 = pairs
  L2_2 = _tStarters
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = L5_2.oStarter
    if L6_2 and not L0_2 then
      L6_2 = L5_2.oStarter
      L0_2 = L6_2.IsContractPending(L6_2)
    end
  end
end

IsContractPending = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = _tStarters[A0_2]
  if not L1_2 then
    return
  end
  L2_2 = {}
  L2_2[1] = L1_2.sBriefingLoc
  L2_2[2] = L1_2.sHero1BriefingLoc
  return L2_2
end

GetStarterBriefingLocs = L0_1

function L0_1(A0_2, A1_2)
  _fLoadCallback = A0_2
  _tLoadCallbackArgs = A1_2
end

SetLoadCallback = L0_1

function L0_1(A0_2, A1_2)
  _fUnloadCallback = A0_2
  _tUnloadCallbackArgs = A1_2
end

SetUnloadCallback = L0_1

function L0_1(A0_2, A1_2)
  _fTeleportCallback = A0_2
  _tTeleportCallbackArgs = A1_2
end

SetTeleportCallback = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L1_2 = Pg.GetGuidByName("Custom Outfit Location")
  if A0_2 then
    Pg.AddContextAction(L1_2, "[ContextAction.ChangeOutfit]", 1)
    L4_2 = {}
    L4_2[1] = 0
    L4_2[2] = L1_2
    _uOutfitEvent = Event.CreatePersistent(Event.ContextAction, L4_2, _SelectOutfit)
    _uOutfitMarker = Marker.AddBlip(L1_2, "HUD_wardrobe", 32, 255, 255, 255, 255, 1.25, 20, 30)
    L3_2 = Net.IsServer()
    if L3_2 then
      Net.SendEvent_AddMarkerObjective(L1_2, _uOutfitMarker, 255, 255, 255, 1.25, MrxUtil.MarkerGetIndexByName_World(L2_2), 1, 16, false, 20, 30)
    end
    L3_2 = MrxUtil.GetPrimaryObjectiveRgb
    L3_2, L4_2, L5_2 = L3_2()
    _uOutfitDisc = Marker.AddDisc(L1_2, 0.5, L3_2, L4_2, L5_2, 0.1)
    L6_2 = Net.IsServer()
    if L6_2 then
      Net.SendEvent_AddMarkerObjective(L1_2, _uOutfitDisc, L3_2, L4_2, L5_2, 0.1, 0, 0.5, 0, true)
    end
  else
    Pg.RemoveContextAction(L1_2)
    Event.Delete(_uOutfitEvent)
    _uOutfitEvent = nil
    L2_2 = _uOutfitMarker
    if L2_2 then
      Marker.Remove(_uOutfitMarker)
      L2_2 = Net.IsServer()
      if L2_2 then
        Net.SendEvent_RemoveMarkerObjective(_uOutfitMarker)
      end
      _uOutfitMarker = nil
    end
    L2_2 = _uOutfitDisc
    if L2_2 then
      Marker.Remove(_uOutfitDisc)
    end
    L2_2 = Net.IsServer()
    if L2_2 then
      Net.SendEvent_RemoveMarkerObjective(_uOutfitDisc)
    end
  end
end

_SetCustomOutfitMarker = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  L0_2 = Pg.GetGuidByName("Custom Outfit Location")
  if not L0_2 then
    return
  end
  L1_2 = Net.IsServer()
  if L1_2 then
    L1_2 = Event.CreatePersistent
    L2_2 = Event.ScriptEvent
    L3_2 = {}
    L4_2 = "mpPlayerLeft"
    
    function L5_2(A0_3)
      local L1_3, L2_3
      L1_3 = Net.IsServer()
      if L1_3 then
        L1_3 = not Player.IsLocal(A0_3[1])
      end
      return L1_3
    end
    
    L3_2[1] = L4_2
    L3_2[2] = L5_2
    _evClientQuitPMC = L1_2(L2_2, L3_2, _ReinitOutfitChange)
  end
  _SetCustomOutfitMarker(true)
end

_InitOutfitChange = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  L0_2 = Pg.GetGuidByName("Custom Outfit Location")
  if not L0_2 then
    return
  end
  _SetCustomOutfitMarker(false)
end

_DeinitOutfitChange = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _WardrobeOpen
  if not L0_2 then
    _DeinitOutfitChange()
    _InitOutfitChange()
  end
end

_ReinitOutfitChange = L0_1
_CostumeDialogBox = nil
_WardrobeOpen = false

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  L1_2 = Player.GetPrimaryCharacter()
  if A0_2 == L1_2 then
    _WardrobeOpen = true
  end
  L1_2 = Net.IsServer()
  if L1_2 then
    _SetCustomOutfitMarker(false)
  end
  L1_2 = Net.IsServer()
  if L1_2 then
    L1_2 = Player.GetSecondaryCharacter()
    if A0_2 == L1_2 then
      Net.SendCustomEvent("WifPmcInterior", NETEVENT_CHANGEOUTFIT, {})
      return
    end
  end
  L2_2 = _tOutfits[MrxUtil.GetCharacterIdentity(A0_2)]
  L3_2 = Player.GetProfileCostume()
  if L3_2 == nil or L3_2 == 0 then
    L3_2 = 0
  end
  L3_2 = L3_2 + 1
  L4_2 = 0
  L5_2 = Net.IsClient()
  if L5_2 then
    L4_2 = Player.GetAvailableCostumes()
  else
    L4_2 = GetAvailableCostumes()
  end
  if L4_2 == nil or L4_2 == 0 then
    L4_2 = 0
  end
  L4_2 = L4_2 + 1
  MrxMultiPageMenu.Reset()
  L5_2 = Net.HasPlayerUnlockedCode()
  if L5_2 and L3_2 ~= 2 then
    L6_2 = L2_2[2].PlayerVisibleName
    L8_2 = {}
    L8_2[1] = A0_2
    L8_2[2] = 2
    MrxMultiPageMenu.AddOption(L6_2, _ChangeOutfit, L8_2)
  end
  L5_2 = ipairs
  L6_2 = L2_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = L9_2.bAlwaysVisible
    if L10_2 and L8_2 ~= L3_2 then
      L13_2 = {}
      L13_2[1] = A0_2
      L13_2[2] = L8_2
      MrxMultiPageMenu.AddOption(L9_2.PlayerVisibleName, _ChangeOutfit, L13_2)
    end
  end
  L5_2 = ipairs
  L6_2 = L2_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    if L8_2 ~= L3_2 and L8_2 ~= 2 and L4_2 >= L8_2 then
      L10_2 = L9_2.bAlwaysVisible
      if not L10_2 then
        L13_2 = {}
        L13_2[1] = A0_2
        L13_2[2] = L8_2
        MrxMultiPageMenu.AddOption(L9_2.PlayerVisibleName, _ChangeOutfit, L13_2)
      end
    end
  end
  L5_2 = Net.IsServer()
  if L5_2 then
    L6_2 = "[Generic.Cancel]"
    L8_2 = {}
    L8_2[1] = true
    MrxMultiPageMenu.AddOption(L6_2, _SetCustomOutfitMarker, L8_2, true, true)
  else
    L5_2 = Net.IsClient()
    if L5_2 then
      L6_2 = "[Generic.Cancel]"
      L8_2 = {}
      L11_2 = {}
      L11_2[1] = 0
      L11_2[2] = 0
      L8_2[1] = "WifPmcInterior"
      L8_2[2] = NETEVENT_NOTIFYOUTFITCHANGE
      L8_2[3] = L11_2
      MrxMultiPageMenu.AddOption(L6_2, Net.SendCustomEvent, L8_2, true, true)
    end
  end
  MrxMultiPageMenu.Display("[Generic.ChooseOutfit]:")
end

_SelectOutfit = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  _CostumeDialogBox = L0_2
  L0_2 = Net.IsServer()
  if L0_2 then
    _SetCustomOutfitMarker(true)
  else
    L0_2 = Net.IsClient()
    if L0_2 then
      L3_2 = {}
      L3_2[1] = 0
      L3_2[2] = 0
      Net.SendCustomEvent("WifPmcInterior", NETEVENT_NOTIFYOUTFITCHANGE, L3_2)
    end
  end
end

_CloseCostumeDialog = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L4_2 = Player.GetPrimaryCharacter()
  if A0_2 == L4_2 then
    _WardrobeOpen = false
  end
  L7_2 = {}
  L7_2[1] = A0_2
  L7_2[2] = A1_2
  L7_2[3] = A2_2
  L7_2[4] = A3_2
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _CompleteChangeOutfit, L7_2)
end

_ChangeOutfit = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L5_2 = _tOutfits[MrxUtil.GetCharacterIdentity(A0_2)]
  L6_2 = L5_2[A1_2].Model
  Player.SetProfileCostume((A1_2 - 1))
  Player.SetOutfit(A0_2, L6_2)
  L7_2 = L5_2[A1_2].Name
  if L7_2 == "Palin" then
    Human.SetChatterSet(A0_2, "Palin_Palin01")
  else
    L7_2 = L5_2[A1_2].Name
    if L7_2 == "Obama" then
      Human.SetChatterSet(A0_2, "Obama_Obama01")
    elseif L4_2 == "mattias" then
      Human.SetChatterSet(A0_2, "Mattias_Peter01")
    elseif L4_2 == "jennifer" then
      Human.SetChatterSet(A0_2, "Jen_Jen01")
    elseif L4_2 == "chris" then
      Human.SetChatterSet(A0_2, "Chris_Phil01")
    end
  end
  L7_2 = Player.GetPrimaryCharacter()
  if A0_2 == L7_2 then
    iPlayer = 1
  else
    L7_2 = Player.GetSecondaryCharacter()
    if A0_2 == L7_2 then
      iPlayer = 2
    end
  end
  L10_2 = {}
  L10_2[1] = iPlayer
  L10_2[2] = A1_2
  Net.SendCustomEvent("WifPmcInterior", NETEVENT_NOTIFYOUTFITCHANGE, L10_2)
  L7_2 = Event.Create
  L8_2 = Event.ObjectIsReady
  L9_2 = {}
  L9_2[1] = A0_2
  L9_2[2] = "awake"
  L9_2[3] = 0
  
  function L10_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3
    L0_3 = _GetPreeningVo(L4_2, L5_2[A1_2].Name)
    L1_3 = nil
    L2_3 = nil
    if L0_3 then
      L1_3 = MrxVoSequence.Start
      L3_3 = {}
      L4_3 = {}
      L5_3 = {}
      L5_3[1] = L0_3
      L5_3[2] = A0_2
      L4_3[1] = L5_3
      L3_3[1] = L4_3
      L3_3[2] = false
      L3_3[3] = MrxVoSequence.knPriorityFreeplay
      L2_3 = L3_3
    end
    _bChangedOutfit = true
    MrxState.Exit(MrxState.STATE_WAITFORGAME, L1_3, L2_3)
    MrxUtil.CallWithOptionalArgs(A2_2, A3_2)
    L3_3 = Net.IsServer()
    if L3_3 then
      _SetCustomOutfitMarker(true)
    end
  end
  
  L7_2(L8_2, L9_2, L10_2)
end

_CompleteChangeOutfit = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  L5_2 = _tOutfits[MrxUtil.GetCharacterIdentity(A0_2)]
  L6_2 = ipairs
  L7_2 = L5_2
  L6_2, L7_2, L8_2 = L6_2(L7_2)
  for L9_2, L10_2 in L6_2, L7_2, L8_2 do
    L11_2 = L10_2.Name
    if L11_2 == A1_2 then
      _ChangeOutfit(A0_2, L9_2, A2_2, A3_2)
      break
    end
  end
end

ChangeOutfit = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _nAvailableCostumes
  if not L0_2 then
    L0_2 = 1
  end
  return L0_2
end

GetAvailableCostumes = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  L3_2 = Player.GetLocalCharacter
  L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2 = L3_2()
  L3_2 = _tOutfits[MrxUtil.GetCharacterIdentity(L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2)]
  if A0_2 < 0 then
    A0_2 = #L3_2 - 1
  end
  if A1_2 then
    L4_2 = 0
    L5_2 = ipairs
    L6_2 = L3_2
    L5_2, L6_2, L7_2 = L5_2(L6_2)
    for L8_2, L9_2 in L5_2, L6_2, L7_2 do
      L10_2 = L9_2.bNoBriefing
      if L10_2 then
        break
      end
      L4_2 = L4_2 + 1
    end
    L4_2 = L4_2 - 1
    if A0_2 > L4_2 then
      L5_2 = {}
      return L5_2
    end
  end
  L4_2 = A0_2 - GetAvailableCostumes()
  _nAvailableCostumes = A0_2
  L5_2 = {}
  if 1 <= L4_2 then
    L6_2 = _nAvailableCostumes + 1
    L8_2 = ((L6_2 - L4_2) + 1)
    L9_2 = L6_2
    L10_2 = 1
    for L11_2 = L8_2, L9_2, L10_2 do
      L12_2 = L3_2[L11_2]
      if L12_2 then
        L13_2 = L12_2.Name
        if L13_2 then
          table.insert(L5_2, L12_2.PlayerVisibleName)
        end
      end
    end
  end
  return L5_2
end

SetAvailableCostumes = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  L2_2 = Player.GetPrimaryCharacter
  L2_2, L3_2 = L2_2()
  L3_2 = _tOutfits[MrxUtil.GetCharacterIdentity(L2_2, L3_2)][A0_2] or L3_2
  if L2_2 then
    L3_2 = not L2_2.bNoBriefing
  end
  return L3_2
end

IsCostumeBriefingSafe = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L1_2 = 1
  L2_2 = 1
  L3_2 = {}
  L4_2 = pairs
  L5_2 = _tStockpile
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    L9_2 = nil
    if L7_2 == "money" then
      L9_2 = MrxPmc.GetCashQty()
    else
      L9_2 = MrxPmc.GetSupportQty(L7_2)
      if L9_2 == nil then
        L9_2 = 0
      end
    end
    L10_2 = _tStockpileQty[L7_2]
    if L10_2 == nil then
      L10_2 = -1
    end
    if L9_2 > L10_2 then
      L11_2 = _tStockpileQty
      L11_2[L7_2] = L9_2
    else
      L9_2 = L10_2
    end
    L3_2[L2_2] = L9_2
    if A0_2 == false then
      _SetStockpileCategoryQty(L7_2, L9_2)
    end
    if L2_2 == 4 then
      L11_2 = Net.IsServer()
      if L11_2 then
        L14_2 = {}
        L16_2 = L3_2[1]
        L17_2 = L3_2[2]
        L18_2 = L3_2[3]
        L19_2 = L3_2[4]
        L14_2[1] = L1_2
        L14_2[2] = L16_2
        L14_2[3] = L17_2
        L14_2[4] = L18_2
        L14_2[5] = L19_2
        Net.SendCustomEvent("WifPmcInterior", NETEVENT_UPDATESTOCKPILE, L14_2, A0_2)
        L3_2[1] = nil
        L3_2[2] = nil
        L3_2[3] = nil
        L3_2[4] = nil
      end
      L2_2 = 1
      L1_2 = L1_2 + 1
    else
      L2_2 = L2_2 + 1
    end
  end
  L4_2 = Net.IsServer()
  if L4_2 and 1 < L2_2 then
    L7_2 = {}
    L9_2 = L3_2[1]
    L10_2 = L3_2[2]
    L11_2 = L3_2[3]
    L12_2 = L3_2[4]
    L7_2[1] = L1_2
    L7_2[2] = L9_2
    L7_2[3] = L10_2
    L7_2[4] = L11_2
    L7_2[5] = L12_2
    Net.SendCustomEvent("WifPmcInterior", NETEVENT_UPDATESTOCKPILE, L7_2, A0_2)
  end
end

_UpdateStockpile = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L2_2 = _tStockpile[A0_2]
  L3_2 = "pmcoutpost_stockpile_" .. A0_2 .. " "
  L4_2 = ipairs
  L5_2 = L2_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    if A1_2 < L8_2 then
      L9_2 = Pg.GetGuidByName((L3_2 .. L7_2))
      if L9_2 then
        L10_2 = Event.Create
        L11_2 = Event.ObjectHibernation
        L12_2 = {}
        L12_2[1] = L9_2
        L12_2[2] = "awake"
        
        function L13_2()
          local L0_3, L1_3, L2_3
          Object.DisablePhysics(L9_2)
          Object.SetVisible(L9_2, false)
        end
        
        L10_2(L11_2, L12_2, L13_2)
      end
    end
  end
end

_SetStockpileCategoryQty = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = pairs
  L2_2 = _tStarters
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    if L4_2 ~= "PmcBoss" then
      L6_2 = L5_2.bIntroduced
      if L6_2 then
        if not L0_2 then
          L0_2 = {}
        end
        L0_2[L4_2] = true
      end
    end
  end
  L1_2 = {}
  L1_2.bUnlocked = _bUnlocked
  L1_2.tStockpileQty = _tStockpileQty
  L1_2.tIntroduced = L0_2
  return L1_2
end

SaveSingleton = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = A0_2.bUnlocked
  if L1_2 then
    Unlock()
    WifPmcGarage.CheckFionaCar(true)
  end
  L1_2 = A0_2.tSupportQty
  if L1_2 then
    _tStockpileQty = A0_2.tStockpileQty
  end
  L1_2 = A0_2.tIntroduced
  if L1_2 then
    L1_2 = pairs
    L2_2 = A0_2.tIntroduced
    L1_2, L2_2, L3_2 = L1_2(L2_2)
    for L4_2, L5_2 in L1_2, L2_2, L3_2 do
      L6_2 = _tStarters[L4_2]
      L6_2.bIntroduced = L5_2
    end
  end
end

LoadSingleton = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  L0_2 = Player.GetSecondaryCharacter()
  if L0_2 then
    L1_2 = Object.IsAwake(L0_2)
    if L1_2 then
      _UpdateStockpile(true)
      _ReinitOutfitChange()
      MrxUtil.EnableHeroWeapons(false)
    else
      L3_2 = {}
      L3_2[1] = L0_2
      L3_2[2] = "awake"
      _evRemotePlayerAwake = Event.Create(Event.ObjectHibernation, L3_2, _OnPlayerJoined)
    end
  else
    L3_2 = {}
    L3_2[1] = 0.3
    _evRemotePlayerAwake = Event.Create(Event.TimerRelative, L3_2, _OnPlayerJoined)
  end
end

_OnPlayerJoined = L0_1
NETEVENT_UPDATESTOCKPILE = 0
NETEVENT_CHANGEOUTFIT = 1
NETEVENT_NOTIFYOUTFITCHANGE = 2
_NetSafeBriefingModule = nil

function L0_1(A0_2)
  local L1_2, L2_2
  _NetSafeBriefingModule = A0_2
  _NetSafeBriefingModule.NetSafeLoadBriefingAssets(_tAssetPreload)
  MrxState.Exit(MrxState.STATE_WAITFORGAME)
end

NetSafeBriefingModuleLoaded = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  dynamic_import("MrxBriefing", NetSafeBriefingModuleLoaded)
  MrxState.Exit(MrxState.STATE_WAITFORGAME)
end

NetSafeLoadAssets1 = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = {}
  L2_2[1] = Player.GetLocalCharacter()
  L2_2[2] = "awake"
  L4_2 = {}
  L4_2[1] = "MrxBriefing"
  L4_2[2] = NetSafeBriefingModuleLoaded
  Event.Create(Event.ObjectHibernation, L2_2, dynamic_import, L4_2)
end

NetSafeLoadAssets = L0_1

function L0_1()
  local L0_2, L1_2
  _ClientExit()
  L0_2 = _NetSafeBriefingModule
  if L0_2 then
    _NetSafeBriefingModule.NetSafeUnloadBriefingAssets(_tAssetPreload)
    dynamic_remove("MrxBriefing")
    _NetSafeBriefingModule = nil
  end
end

NetSafeUnloadAssets = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L2_2 = NETEVENT_UPDATESTOCKPILE
  if A0_2 == L2_2 then
    L2_2 = 1
    L3_2 = nil
    L4_2 = 1
    L5_2 = pairs
    L6_2 = _tStockpile
    L5_2, L6_2, L7_2 = L5_2(L6_2)
    for L8_2, L9_2 in L5_2, L6_2, L7_2 do
      L10_2 = A1_2[1]
      if L2_2 == L10_2 and not L3_2 then
        L3_2 = 1
      end
      if L3_2 then
        L10_2 = A1_2[(1 + L3_2)]
        if L10_2 then
          _SetStockpileCategoryQty(L8_2, A1_2[(1 + L3_2)])
          L3_2 = L3_2 + 1
        end
      end
      L4_2 = L4_2 + 1
      if L4_2 == 5 then
        L2_2 = L2_2 + 1
        L4_2 = 1
        L3_2 = nil
      end
    end
  else
    L2_2 = NETEVENT_CHANGEOUTFIT
    if A0_2 == L2_2 then
      L3_2 = Player.GetLocalCharacter
      L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2 = L3_2()
      _SelectOutfit(L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2)
    else
      L2_2 = NETEVENT_NOTIFYOUTFITCHANGE
      if A0_2 == L2_2 then
        L2_2 = 0
        L3_2 = A1_2[1]
        if L3_2 == 0 then
          _SetCustomOutfitMarker(true)
          return
        else
          L3_2 = A1_2[1]
          if L3_2 == 1 then
            L2_2 = Player.GetPrimaryCharacter()
          else
            L3_2 = A1_2[1]
            if L3_2 == 2 then
              L2_2 = Player.GetSecondaryCharacter()
            end
          end
        end
        Player.SetOutfit(L2_2, _tOutfits[MrxUtil.GetCharacterIdentity(L2_2)][A1_2[2]].Model)
        L6_2 = Net.IsServer()
        if L6_2 then
          _SetCustomOutfitMarker(true)
        end
        L6_2 = Net.IsClient()
        if L6_2 then
          MrxPlayer.SetRemoteOutfit((A1_2[2] - 1))
        end
      end
    end
  end
end

NetEventCallback = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  _SetFakePDA(true)
  Graphics.Camera.SetNearFar(0, 0.3, 500, 0)
  MrxGuiInterface.HudInterface.FanfareQueue.ClientPause(false)
end

_ClientOnEnter = L0_1

function L0_1()
  local L0_2, L1_2
  MrxMultiPageMenu.Close()
  L0_2 = _CostumeDialogBox
  if L0_2 then
    MrxGui.CloseDialogBox(_CostumeDialogBox)
    _CostumeDialogBox = nil
  end
  MrxHq.GlobalExit()
  _SetFakePDA(false)
  Graphics.Camera.RestoreNearFar(0)
end

_ClientExit = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  L4_2 = {}
  L4_2[1] = A0_2
  Net.SendCustomEvent("WifPmcInterior", NETEVENT_UPDATESTOCKPILE, L4_2)
end

_SendStockpileToClient = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  Object.DisablePhysics(A0_2)
  Object.SetVisible(A0_2, false)
end

_ClientUpdateStockpile = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = _bPreOilCon020
  if not L1_2 then
    return
  end
  if A0_2 == nil then
    A0_2 = 5
  end
  L3_2 = {}
  L3_2[1] = A0_2
  _uNagTimer = Event.Create(Event.TimerRelative, L3_2, _PreOilCon020Nag)
end

_SetupPreOilCon020Nag = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = _uNagTimer
  if L0_2 then
    Event.Delete(_uNagTimer)
    _uNagTimer = nil
  end
  L0_2 = _bNagInProgress
  if L0_2 then
    VO.Cancel(_tStarters.PmcBoss.uStarter)
  end
end

_StopPreOilCon020Nag = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  
  function L0_2(A0_3)
    local L1_3, L2_3
    _bNagInProgress = false
    if A0_3 == "complete" then
      _SetupPreOilCon020Nag(30)
    end
  end
  
  _StopPreOilCon020Nag()
  _bNagInProgress = true
  L1_2 = {}
  L1_2[1] = "Fiona.AtPMC.01"
  L1_2[2] = "Fiona.AtPMC.02"
  L1_2[3] = "Fiona.AtPMC.03"
  L1_2[4] = "Fiona.AtPMC.04"
  L1_2[5] = "Fiona.AtPMC.05"
  VO.Cue(_tStarters.PmcBoss.uStarter, MrxUtil.GetRandomTableElement(L1_2), L0_2)
end

_PreOilCon020Nag = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = pairs
  L2_2 = _tStarters
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    _SetStarterMarker(L4_2, A0_2)
  end
  L1_2 = pairs
  L2_2 = _tPortals
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    _SetPortalMarker(L4_2, A0_2)
  end
end

_SetAllMarkers = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L1_2 = {}
  L2_2 = {}
  L3_2 = {}
  L3_2[1] = "Fiona.Misc.CustomOutfit01"
  L2_2.CustomOutfit = L3_2
  L3_2 = {}
  L3_2[1] = "Fiona.PMC.Greeting05"
  L3_2[2] = "Fiona.PMC.Greeting06"
  L3_2[3] = "Fiona.PMC.Greeting07"
  L3_2[4] = "Fiona.PMC.Greeting08"
  L3_2[5] = "Fiona.PMC.Greeting09"
  L2_2.Greeting = L3_2
  L3_2 = {}
  L4_2 = {}
  L4_2[1] = "Fiona.Cam.57"
  L3_2.HelPmcBoss = L4_2
  L4_2 = {}
  L4_2[1] = "Fiona.Cam.58"
  L3_2.MecPmcBoss = L4_2
  L4_2 = {}
  L4_2[1] = "Fiona.Cam.59"
  L3_2.JetPmcBoss = L4_2
  L2_2.Intros = L3_2
  L1_2.PmcBoss = L2_2
  L2_2 = {}
  L3_2 = {}
  L3_2[1] = "Ewan.Misc.CustomOutfit01"
  L2_2.CustomOutfit = L3_2
  L3_2 = {}
  L3_2[1] = "Ewan.PMC.Greeting01"
  L3_2[2] = "Ewan.PMC.Greeting02"
  L3_2[3] = "Ewan.PMC.Greeting03"
  L3_2[4] = "Ewan.PMC.Greeting04"
  L3_2[5] = "Ewan.PMC.Greeting05"
  L3_2[6] = "Ewan.PMC.Greeting06"
  L3_2[7] = "Ewan.PMC.Greeting07"
  L3_2[8] = "Ewan.PMC.Greeting08"
  L3_2[9] = "Ewan.PMC.Greeting09"
  L3_2[10] = "Ewan.PMC.Greeting10"
  L2_2.Greeting = L3_2
  L1_2.HelPmcBoss = L2_2
  L2_2 = {}
  L3_2 = {}
  L3_2[1] = "Eva.Misc.CustomOutfit01"
  L2_2.CustomOutfit = L3_2
  L3_2 = {}
  L3_2[1] = "Eva.PMC.Greeting01"
  L3_2[2] = "Eva.PMC.Greeting02"
  L3_2[3] = "Eva.PMC.Greeting03"
  L3_2[4] = "Eva.PMC.Greeting04"
  L3_2[5] = "Eva.PMC.Greeting05"
  L3_2[6] = "Eva.PMC.Greeting07"
  L3_2[7] = "Eva.PMC.Greeting08"
  L3_2[8] = "Eva.PMC.Greeting09"
  L3_2[9] = "Eva.PMC.Greeting10"
  L2_2.Greeting = L3_2
  L1_2.MecPmcBoss = L2_2
  L2_2 = {}
  L3_2 = {}
  L3_2[1] = "Misha.Misc.CustomOutfit01"
  L2_2.CustomOutfit = L3_2
  L3_2 = {}
  L3_2[1] = "Misha.PMC.Greeting01"
  L3_2[2] = "Misha.PMC.Greeting02"
  L3_2[3] = "Misha.PMC.Greeting03"
  L3_2[4] = "Misha.PMC.Greeting04"
  L3_2[5] = "Misha.PMC.Greeting05"
  L3_2[6] = "Misha.PMC.Greeting06"
  L3_2[7] = "Misha.PMC.Greeting07"
  L3_2[8] = "Misha.PMC.Greeting08"
  L3_2[9] = "Misha.PMC.Greeting09"
  L3_2[10] = "Misha.PMC.Greeting10"
  L2_2.Greeting = L3_2
  L1_2.JetPmcBoss = L2_2
  L2_2 = true
  L3_2 = L1_2[A0_2]
  L4_2 = nil
  L5_2 = _bChangedOutfit
  if L5_2 then
    L4_2 = L3_2.CustomOutfit
    _bChangedOutfit = nil
  end
  if A0_2 == "PmcBoss" then
    L5_2 = pairs
    L6_2 = _tStarters
    L5_2, L6_2, L7_2 = L5_2(L6_2)
    for L8_2, L9_2 in L5_2, L6_2, L7_2 do
      if L8_2 ~= "PmcBoss" then
        L10_2 = L9_2.oStarter
        if L10_2 then
          L10_2 = L9_2.bIntroduced
          if not L10_2 then
            L4_2 = L3_2.Intros[L8_2]
            L9_2.bIntroduced = true
            break
          end
        end
      end
    end
  end
  if not L4_2 then
    L2_2 = false
    L4_2 = L3_2.Greeting
    if A0_2 == "PmcBoss" then
      L5_2 = _tStarters.MecPmcBoss.oStarter
      if L5_2 then
        L5_2 = _tStarters.HelPmcBoss.oStarter
        if L5_2 then
          table.insert(L4_2, "Fiona.PMC.Greeting02")
        end
      end
      L5_2 = MrxPmc.GetCashQty()
      if 1000000 < L5_2 then
        table.insert(L4_2, "Fiona.PMC.Greeting10")
      end
      L5_2 = WifMissionFlow.HasKey("PmcCon002")
      if L5_2 then
        table.insert(L4_2, "Fiona.PMC.Greeting01")
      end
      L5_2 = _bWelcomedPlayer
      if not L5_2 then
        L5_2 = WifMissionFlow.HasKey("PmcCon002")
        if not L5_2 then
          table.insert(L4_2, "Fiona.PMC.Greeting04")
        end
        L5_2 = MrxPmc.GetCashQty()
        if 1000000 < L5_2 then
          table.insert(L4_2, "Fiona.PMC.Greeting03")
        end
        _bWelcomedPlayer = true
      end
    elseif A0_2 == "MecPmcBoss" then
      L5_2 = _tStarters.JetPmcBoss.oStarter
      if L5_2 then
        table.insert(L4_2, "Eva.PMC.Greeting06")
      end
    end
  end
  if not L4_2 then
    L5_2 = nil
    return L5_2
  end
  if L2_2 then
    L5_2 = MrxUtil.GetRandomTableElement
    L6_2 = L4_2
    return L5_2(L6_2)
  end
  L5_2 = _tStarters[A0_2].nLastChatter
  if L5_2 ~= nil then
    L6_2 = table.getn(L4_2)
    if not (L5_2 > L6_2) then
      goto lbl_220
    end
  end
  L5_2 = 1
  goto lbl_225
  ::lbl_220::
  L6_2 = L4_2[(L5_2 + 1)]
  if not L6_2 then
    L5_2 = 1
  end
  ::lbl_225::
  L6_2 = _tStarters[A0_2]
  L6_2.nLastChatter = L5_2
  L6_2 = L4_2[L5_2]
  return L6_2
end

_GetStarterChatterVo = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = pairs
  L2_2 = _tStarters
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    _SetStarterChatter(L4_2, A0_2)
  end
end

_SetAllStarterChatter = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = {}
  L3_2 = {}
  L4_2 = {}
  L4_2[1] = "Mattias.CustomOutfit.01"
  L4_2[2] = "Mattias.CustomOutfit.02"
  L4_2[3] = "Mattias.CustomOutfit.03"
  L4_2[4] = "Mattias.CustomOutfit.04"
  L3_2.Original = L4_2
  L4_2 = {}
  L4_2[1] = "Mattias.CustomOutfit.Chicken.01"
  L4_2[2] = "Mattias.CustomOutfit.Chicken.02"
  L4_2[3] = "Mattias.CustomOutfit.Chicken.03"
  L4_2[4] = "Mattias.CustomOutfit.Chicken.04"
  L4_2[5] = "Mattias.CustomOutfit.Chicken.05"
  L3_2.ChickenSuit = L4_2
  L4_2 = {}
  L4_2[1] = "Mattias.CustomOutfit.01"
  L4_2[2] = "Mattias.CustomOutfit.02"
  L4_2[3] = "Mattias.CustomOutfit.03"
  L4_2[4] = "Mattias.CustomOutfit.04"
  L3_2.Suit = L4_2
  L4_2 = {}
  L4_2[1] = "Mattias.CustomOutfit.01"
  L4_2[2] = "Mattias.CustomOutfit.02"
  L4_2[3] = "Mattias.CustomOutfit.03"
  L4_2[4] = "Mattias.CustomOutfit.04"
  L3_2.MetalHead = L4_2
  L4_2 = {}
  L4_2[1] = "Mattias.CustomOutfit.01"
  L4_2[2] = "Mattias.CustomOutfit.02"
  L4_2[3] = "Mattias.CustomOutfit.03"
  L4_2[4] = "Mattias.CustomOutfit.04"
  L3_2.Jacket = L4_2
  L2_2.mattias = L3_2
  L3_2 = {}
  L4_2 = {}
  L4_2[1] = "Chris.CustomOutfit.Generic01"
  L3_2.Original = L4_2
  L4_2 = {}
  L4_2[1] = "Chris.CustomOutfit.Chicken.01"
  L4_2[2] = "Chris.CustomOutfit.Chicken.02"
  L4_2[3] = "Chris.CustomOutfit.Chicken.03"
  L4_2[4] = "Chris.CustomOutfit.Chicken.04"
  L4_2[5] = "Chris.CustomOutfit.Chicken.05"
  L3_2.ChickenSuit = L4_2
  L4_2 = {}
  L4_2[1] = "Chris.CustomOutfit.Magnum01"
  L4_2[2] = "Chris.CustomOutfit.Magnum02"
  L4_2[3] = "Chris.CustomOutfit.Generic01"
  L3_2.Vacation = L4_2
  L4_2 = {}
  L4_2[1] = "Chris.CustomOutfit.Rambo01"
  L4_2[2] = "Chris.CustomOutfit.Rambo02"
  L4_2[3] = "Chris.CustomOutfit.Generic01"
  L3_2.Commando = L4_2
  L4_2 = {}
  L4_2[1] = "Chris.CustomOutfit.Magnum02"
  L4_2[2] = "Chris.CustomOutfit.Generic01"
  L3_2.OffDuty = L4_2
  L2_2.chris = L3_2
  L3_2 = {}
  L4_2 = {}
  L4_2[1] = "Jen.CustomOutfit.Styleish01"
  L3_2.Original = L4_2
  L4_2 = {}
  L4_2[1] = "Jen.CustomOutfit.Chicken.01"
  L4_2[2] = "Jen.CustomOutfit.Chicken.02"
  L4_2[3] = "Jen.CustomOutfit.Chicken.03"
  L4_2[4] = "Jen.CustomOutfit.Chicken.04"
  L4_2[5] = "Jen.CustomOutfit.Chicken.05"
  L3_2.ChickenSuit = L4_2
  L4_2 = {}
  L4_2[1] = "Jen.CustomOutfit.Army01"
  L3_2.Rebel = L4_2
  L4_2 = {}
  L4_2[1] = "Jen.CustomOutfit.Catsuit01"
  L4_2[2] = "Jen.CustomOutfit.Styleish01"
  L3_2.CatSuit = L4_2
  L4_2 = {}
  L4_2[1] = "Jen.CustomOutfit.Jean01"
  L4_2[2] = "Jen.CustomOutfit.Styleish01"
  L3_2.NoJacket = L4_2
  L4_2 = {}
  L4_2[1] = "Jen.CustomOutfit.Army01"
  L4_2[2] = "Jen.CustomOutfit.Styleish01"
  L3_2.Tactical = L4_2
  L2_2.jennifer = L3_2
  L3_2 = L2_2[A0_2]
  if A1_2 == "Chicken Suit" then
    A1_2 = "ChickenSuit"
  elseif A1_2 == "NoJacket" then
    A1_2 = "NoJacket"
  elseif A1_2 == "Cat Suit" then
    A1_2 = "CatSuit"
  end
  L4_2 = L3_2[A1_2]
  if not L4_2 then
    return
  end
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = L4_2
  return L5_2(L6_2)
end

_GetPreeningVo = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = _tBuildingStates[A0_2]
  if not L1_2 then
    return
  end
  L1_2 = Pg.GetGuidByName(_tBuildings[A0_2])
  L2_2 = _tBuildingStates
  L2_2[A0_2] = false
  if A0_2 == 1 then
    _EnablePortals(false, false)
  elseif A0_2 == 2 then
    _SetPmcTransitLocation(false)
  end
  L2_2 = _tBuildingEvents
  L5_2 = {}
  L5_2[1] = L1_2
  L5_2[2] = "s"
  L7_2 = {}
  L7_2[1] = A0_2
  L2_2[A0_2] = Event.Create(Event.ObjectHibernation, L5_2, _OnPmcHibernation, L7_2)
end

_OnPmcDeath = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = _tBuildingStates[A0_2]
  if L2_2 then
    return
  end
  L2_2 = Pg.GetGuidByName(_tBuildings[A0_2])
  L3_2 = _tBuildingStates
  L3_2[A0_2] = true
  if not A1_2 then
    Object.Revive(L2_2)
    if A0_2 == 1 then
      _EnablePortals(true, false)
    elseif A0_2 == 2 then
      _SetPmcTransitLocation(true)
    end
  end
  L3_2 = _tBuildingEvents
  L6_2 = {}
  L6_2[1] = L2_2
  L8_2 = {}
  L8_2[1] = A0_2
  L3_2[A0_2] = Event.Create(Event.ObjectDeath, L6_2, _OnPmcDeath, L8_2)
end

_OnPmcHibernation = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  MrxTransit.SuppressLocation(1, not A0_2)
end

_SetPmcTransitLocation = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L2_2 = MrxRewardData.GetRewards(WifMissionFlow.GetCaseSensitiveMissionId(A0_2))
  if L2_2 then
    L3_2 = MrxRewardData.GetWagerData(L2_2)
    if L3_2 then
      L6_2 = {}
      L6_2[1] = false
      Event.Post("parkingLotStart", L6_2)
      return
    end
  end
  L3_2 = Pg.GetGuidByName("01_pmc_hq_parking")
  L4_2 = Pg.GetGuidByName("01_pmc_hq_lz_playerone")
  L5_2 = _tPortalData[_nPortal]
  L8_2 = {}
  L8_2[1] = Pg.GetGuidByName(L5_2.sExterior_Entrance)
  L8_2[2] = L3_2
  L8_2[3] = L4_2
  Event.Post("parkingLotStart", L8_2)
end

_DoParkingLot = L0_1
