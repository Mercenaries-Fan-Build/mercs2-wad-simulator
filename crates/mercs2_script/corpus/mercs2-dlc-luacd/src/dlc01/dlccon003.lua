local L0_1, L1_1, L2_1
inherit("MrxTaskContract", false)
import("MrxLayerManager", false)
import("MrxVoSequence", false)
import("MrxFactionManager", false)
import("MrxUtil", false)
import("MrxPmc", false)
import("MrxGuiInterface", false)
import("DlcVehicleStrike", false)
import("WifVzBoundary", false)
import("MrxMusic", false)
import("MrxTutorialManager", false)
import("DLC01_MissionHub", false)

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = {}
  L1_2[1] = "DLC01_state_DLCCon003"
  L1_2[2] = "DLC01_state_DLCCon003_Pathfinding"
  L1_2[3] = "DLC01_state_DLCCon003_Spawns"
  L1_2[4] = "DLC01_state_DLCCon003_AtmoFX"
  L5_2 = {}
  L5_2[1] = A0_2
  MrxLayerManager.Add(L1_2, InitPlayerSpawn, L5_2)
end

LoadAssets = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  tSpawned = {}
  L1_2 = Pg.GetGuidByName("DLC_AT Rocket")
  L2_2 = Pg.GetGuidByName("Combat Rifle")
  L3_2 = Pg.GetGuidByName("C4")
  L4_2 = Pg.GetGuidByName("Grenade")
  uPlayerOne = Player.GetPrimaryCharacter()
  uPlayerTwo = Player.GetSecondaryCharacter()
  L5_2 = Player.GetCurrentPlayers()
  if L5_2 == 2 then
    L5_2 = MrxUtil.SpawnObject("DLC_M1A3", "DLCCon003_Player01_Tank_loc")
    L8_2 = {}
    L8_2[1] = L1_2
    L8_2[2] = L2_2
    L8_2[3] = L3_2
    L8_2[4] = L4_2
    Human.Inventory.SetAllWeapons(uPlayerOne, L8_2)
    L6_2 = MrxUtil.SpawnObject("DLC_M1A3", "DLCCon003_Player02_Tank_loc")
    L9_2 = {}
    L9_2[1] = L1_2
    L9_2[2] = L2_2
    L9_2[3] = L3_2
    L9_2[4] = L4_2
    Human.Inventory.SetAllWeapons(uPlayerTwo, L9_2)
    table.insert(tSpawned, L5_2)
    table.insert(tSpawned, L6_2)
  else
    L5_2 = MrxUtil.SpawnObject("DLC_M1A3", "DLCCon003_Player01_Tank_loc")
    L8_2 = {}
    L8_2[1] = L1_2
    L8_2[2] = L2_2
    L8_2[3] = L3_2
    L8_2[4] = L4_2
    Human.Inventory.SetAllWeapons(uPlayerOne, L8_2)
    table.insert(tSpawned, L5_2)
  end
  DlcVehicleStrike.CreateTankSupport()
  L5_2 = DlcVehicleStrike
  a = L5_2.Create(L5_2)
  L5_2 = a
  L5_2.AddStrike(L5_2)
  L7_2 = false
  WifVzBoundary.SetupBoundary("DLCCon003_MissionBoundary", L7_2)
  bHero1TankReady = nil
  bHero2TankReady = nil
  L5_2 = pairs
  L6_2 = tSpawned
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L13_2 = {}
    L13_2[1] = L9_2
    L13_2[2] = "awake"
    L15_2 = {}
    L15_2[1] = A0_2
    L15_2[2] = tSpawned
    L15_2[3] = L9_2
    A0_2._CreateEvent(A0_2, Event.ObjectHibernation, L13_2, A0_2.EnsureTanksReady, L15_2)
  end
end

InitPlayerSpawn = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2
  L3_2 = A1_2[1]
  L4_2 = A1_2[2]
  if A2_2 == L3_2 then
    bHero1TankReady = true
  elseif A2_2 == L4_2 then
    bHero2TankReady = true
  end
  L5_2 = bHero1TankReady
  if L5_2 or not L3_2 then
    L5_2 = bHero2TankReady
    if L5_2 or not L4_2 then
      bHero1TankReady = nil
      bHero2TankReady = nil
      A0_2.PutHeroesInTanks(A0_2, A1_2)
    end
  end
end

EnsureTanksReady = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L2_2 = Player.GetPrimaryCharacter()
  L3_2 = Player.GetSecondaryCharacter()
  L4_2 = A1_2[1]
  L5_2 = A1_2[2]
  bHero1InTank = nil
  bHero2InTank = nil
  if L2_2 and L4_2 then
    Vehicle.Enter(L4_2, L2_2, "d", true, false)
    L9_2 = {}
    L9_2[1] = L2_2
    L9_2[2] = L4_2
    L9_2[3] = "D"
    L9_2[4] = "E"
    L11_2 = {}
    L11_2[1] = A0_2
    A0_2._CreateEvent(A0_2, Event.ObjectInSeat, L9_2, A0_2.EnsureHeroesInTanks, L11_2)
  end
  if L3_2 and L5_2 then
    Vehicle.Enter(L5_2, L3_2, "d", true, false)
    L9_2 = {}
    L9_2[1] = L3_2
    L9_2[2] = L5_2
    L9_2[3] = "D"
    L9_2[4] = "E"
    L11_2 = {}
    L11_2[1] = A0_2
    A0_2._CreateEvent(A0_2, Event.ObjectInSeat, L9_2, A0_2.EnsureHeroesInTanks, L11_2)
  end
end

PutHeroesInTanks = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L2_2 = Player.GetPrimaryCharacter()
  L3_2 = Player.GetSecondaryCharacter()
  if L2_2 == A1_2 then
    bHero1InTank = true
  elseif L3_2 == A1_2 then
    bHero2InTank = true
  end
  L4_2 = bHero1InTank
  if L4_2 or not L2_2 then
    L4_2 = bHero2InTank
    if L4_2 or not L3_2 then
      L4_2 = Player.GetAllPlayers()
      L5_2 = ipairs
      L6_2 = L4_2
      L5_2, L6_2, L7_2 = L5_2(L6_2)
      for L8_2, L9_2 in L5_2, L6_2, L7_2 do
        L10_2 = Player.GetCamera(L9_2)
        if L10_2 then
          Camera.StopBlending(L10_2)
        end
      end
      bHero1InTank = nil
      bHero2InTank = nil
      A0_2.AssetsLoaded(A0_2)
    end
  end
end

EnsureHeroesInTanks = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2, L53_2, L54_2, L55_2
  L1_2 = Player.GetLocalCharacter()
  L2_2 = Player.GetPrimaryPlayer()
  L3_2 = Player.GetSecondaryPlayer()
  L4_2 = Player.GetCurrentPlayers()
  uChiVehFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(uChiVehFilter, "China && Tank")
  nPlayerTanks = 0
  nBonusKills = 0
  nGiveTB = 0
  nSpawn = 0
  nLive = 0
  nGoal = 0
  nTankDeath = 0
  nPlyrLoc = 0
  nCurTankHealth = 0
  bFirstPickup = true
  nRound = 1
  nFailCon = 10
  nTankHealth = 3
  nVicCon = 60
  nSpwnTime = 45
  nAdv = 0.5
  nRoundValue = 21
  nTime = 10
  nTBSpwnBase = 5
  nEscPen = {}
  L5_2 = nEscPen
  L5_2[Pg.GetGuidByName("PLZ45 (DLC) (LongHib) (Prototype)")] = 100000
  L5_2 = nEscPen
  L5_2[Pg.GetGuidByName("ZTZ63a (DLC) (LongHib) (Prototype)")] = 200000
  L5_2 = nEscPen
  L5_2[Pg.GetGuidByName("ZTZ98 (DLC) (LongHib) (Prototype)")] = 500000
  nEscPenTime = 20
  nKillBonus = {}
  L5_2 = nKillBonus
  L5_2[Pg.GetGuidByName("PLZ45 (DLC) (LongHib) (Prototype)")] = 100000
  L5_2 = nKillBonus
  L5_2[Pg.GetGuidByName("ZTZ63a (DLC) (LongHib) (Prototype)")] = 200000
  L5_2 = nKillBonus
  L5_2[Pg.GetGuidByName("ZTZ98 (DLC) (LongHib) (Prototype)")] = 500000
  knHeliBonus = 1000000
  A0_2.nTankMoney = 0
  A0_2.nEscapeMoney = 0
  nRepairMoney = 0
  A0_2.nHeliBonusMoney = 0
  A0_2.tVehicleEvents = {}
  A0_2.tExitRegionCounters = {}
  nTBSpawn = nTBSpwnBase
  L5_2 = {}
  L9_2 = 7
  L10_2 = 9
  L5_2[1] = 1
  L5_2[2] = 3
  L5_2[3] = 5
  L5_2[4] = L9_2
  L5_2[5] = L10_2
  tTracerTime = L5_2
  L5_2 = {}
  L5_2[1] = "DLCCon003_ExitLoc_01"
  L5_2[2] = "DLCCon003_ExitLoc_02"
  L5_2[3] = "DLCCon003_ExitLoc_03"
  tExitLocs = L5_2
  tExitLocMarkers = {}
  L5_2 = {}
  L6_2 = Pg.GetGuidByName("DLCCon003_overpass_a")
  L7_2 = Pg.GetGuidByName
  L8_2 = "DLCCon003_overpass_b"
  L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2, L53_2, L54_2, L55_2 = L7_2(L8_2)
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L5_2[3] = L8_2
  L5_2[4] = L9_2
  L5_2[5] = L10_2
  L5_2[6] = L11_2
  L5_2[7] = L12_2
  L5_2[8] = L13_2
  L5_2[9] = L14_2
  L5_2[10] = L15_2
  L5_2[11] = L16_2
  L5_2[12] = L17_2
  L5_2[13] = L18_2
  L5_2[14] = L19_2
  L5_2[15] = L20_2
  L5_2[16] = L21_2
  L5_2[17] = L22_2
  L5_2[18] = L23_2
  L5_2[19] = L24_2
  L5_2[20] = L25_2
  L5_2[21] = L26_2
  L5_2[22] = L27_2
  L5_2[23] = L28_2
  L5_2[24] = L29_2
  L5_2[25] = L30_2
  L5_2[26] = L31_2
  L5_2[27] = L32_2
  L5_2[28] = L33_2
  L5_2[29] = L34_2
  L5_2[30] = L35_2
  L5_2[31] = L36_2
  L5_2[32] = L37_2
  L5_2[33] = L38_2
  L5_2[34] = L39_2
  L5_2[35] = L40_2
  L5_2[36] = L41_2
  L5_2[37] = L42_2
  L5_2[38] = L43_2
  L5_2[39] = L44_2
  L5_2[40] = L45_2
  L5_2[41] = L46_2
  L5_2[42] = L47_2
  L5_2[43] = L48_2
  L5_2[44] = L49_2
  L5_2[45] = L50_2
  L5_2[46] = L51_2
  L5_2[47] = L52_2
  L5_2[48] = L53_2
  L5_2[49] = L54_2
  L5_2[50] = L55_2
  tOverpasses = L5_2
  L5_2 = {}
  L5_2[1] = "DLCCon003_bridge01a"
  L5_2[2] = "DLCCon003_bridge01b"
  L5_2[3] = "DLCCon003_bridge01c"
  L5_2[4] = "DLCCon003_bridge01d"
  L5_2[5] = "DLCCon003_bridge01e"
  L5_2[6] = "DLCCon003_bridge01f"
  L5_2[7] = "DLCCon003_bridge01g"
  L5_2[8] = "DLCCon003_bridge02a"
  L5_2[9] = "DLCCon003_bridge02b"
  L5_2[10] = "DLCCon003_bridge02c"
  L5_2[11] = "DLCCon003_bridge02d"
  L5_2[12] = "DLCCon003_bridge02e"
  L5_2[13] = "DLCCon003_bridge02f"
  L5_2[14] = "DLCCon003_bridge02g"
  L5_2[15] = "DLCCon003_bridge03a"
  L5_2[16] = "DLCCon003_bridge03b"
  L5_2[17] = "DLCCon003_bridge03c"
  L5_2[18] = "DLCCon003_bridge03d"
  L5_2[19] = "DLCCon003_bridge03e"
  L5_2[20] = "DLCCon003_bridge03f"
  tBridges = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x01_B07"
  tX01 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x02_B07"
  tX02 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x03_B04"
  tX03 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x04_B03"
  tX04 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x05_D06"
  tX05 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x06_E05"
  tX06 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x07_D04"
  tX07 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x08_D04"
  tX08 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x09_E05"
  tX09 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x10_G07"
  tX10 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x11_G07"
  tX11 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x12_G07"
  tX12 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x13_G05"
  tX13 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_x14_G05"
  tX14 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_B01_D01"
  L5_2[2] = "Path_B01_A05"
  L5_2[3] = "Path_B01_B02"
  L5_2[4] = "Path_B01_B02"
  L5_2[5] = "Path_B01_B02"
  tB01 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_D01_B01"
  L5_2[2] = "Path_D01_D02"
  L5_2[3] = "Path_D01_D02"
  L5_2[4] = "Path_D01_D02"
  tD01 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_E01_F01"
  L5_2[2] = "Path_E01_E02"
  L5_2[3] = "Path_E01_E02"
  L5_2[4] = "Path_E01_E02"
  tE01 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_F01_G02"
  L5_2[2] = "Path_F01_E01"
  L5_2[3] = "Path_F01_F02"
  L5_2[4] = "Path_F01_F02"
  L5_2[5] = "Path_F01_F02"
  tF01 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_B02_D02"
  L5_2[2] = "Path_B02_B03"
  L5_2[3] = "Path_B02_B03"
  L5_2[4] = "Path_B02_B03"
  tB02 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_D02_E02"
  L5_2[2] = "Path_D02_B02"
  L5_2[3] = "Path_D02_D04"
  L5_2[4] = "Path_D02_D04"
  L5_2[5] = "Path_D02_D04"
  tD02 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_E02_F02"
  L5_2[2] = "Path_E02_D02"
  L5_2[3] = "Path_E01_E02"
  tE02 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_F02_G02"
  L5_2[2] = "Path_F02_E02"
  tF02 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_G02_F02"
  L5_2[2] = "Path_G02_G03"
  L5_2[3] = "Path_G02_G03"
  L5_2[4] = "Path_G02_G03"
  tG02 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_B03_A05"
  L5_2[2] = "Path_B03_B04"
  L5_2[3] = "Path_B03_B04"
  L5_2[4] = "Path_B03_B04"
  tB03 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_G03_H05"
  L5_2[2] = "Path_G03_G05"
  L5_2[3] = "Path_G03_G05"
  L5_2[4] = "Path_G03_G05"
  tG03 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_B04_D04"
  L5_2[2] = "Path_B04_B06"
  L5_2[3] = "Path_B04_B06"
  L5_2[4] = "Path_B04_B06"
  tB04 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_D04_B04"
  L5_2[2] = "Path_D04_D05"
  L5_2[3] = "Path_D04_D05"
  L5_2[4] = "Path_D04_D05"
  tD04 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_A05_B07"
  tA05 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_D05_E05"
  L5_2[2] = "Path_D05_D06"
  L5_2[3] = "Path_D05_D06"
  L5_2[4] = "Path_D05_D06"
  tD05 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_E05_D05"
  L5_2[2] = "Path_E05_E08"
  L5_2[3] = "Path_E05_E08"
  L5_2[4] = "Path_E05_E08"
  tE05 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_G05_G07"
  L5_2[2] = "Path_G05_G07"
  L5_2[3] = "Path_G05_G07"
  tG05 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_H05_G05"
  L5_2[2] = "Path_H05_G07"
  L5_2[3] = "Path_H05_G07"
  L5_2[4] = "Path_H05_G07"
  tH05 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_B06_D06"
  L5_2[2] = "Path_B06_B07"
  L5_2[3] = "Path_B06_B07"
  L5_2[4] = "Path_B06_B07"
  tB06 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_D06_B06"
  L5_2[2] = "Path_D06_D08"
  L5_2[3] = "Path_D06_D08"
  L5_2[4] = "Path_D06_D08"
  tD06 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_B07_B08"
  L5_2[2] = "Path_B07_B08"
  L5_2[3] = "Path_B07_B08"
  tB07 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_G07_G08"
  L5_2[2] = "Path_G07_G08"
  L5_2[3] = "Path_G07_G08"
  tG07 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_B08_C08"
  L5_2[2] = "Path_B08_B09"
  L5_2[3] = "Path_B08_B09"
  L5_2[4] = "Path_B08_B09"
  L5_2[5] = "Path_B08_B09"
  tB08 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_C08_D08"
  L5_2[2] = "Path_C08_B08"
  L5_2[3] = "Path_C08_C09"
  L5_2[4] = "Path_C08_C09"
  L5_2[5] = "Path_C08_C09"
  tC08 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_D08_E08"
  L5_2[2] = "Path_D08_C08"
  L5_2[3] = "Path_D08_D09"
  L5_2[4] = "Path_D08_D09"
  L5_2[5] = "Path_D08_D09"
  tD08 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_E08_F08"
  L5_2[2] = "Path_E08_D08"
  L5_2[3] = "Path_E08_E09"
  L5_2[4] = "Path_E08_E09"
  L5_2[5] = "Path_E08_E09"
  tE08 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_F08_G08"
  L5_2[2] = "Path_F08_E08"
  L5_2[3] = "Path_F08_F10"
  L5_2[4] = "Path_F08_F10"
  L5_2[5] = "Path_F08_F10"
  tF08 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_G08_F08"
  L5_2[2] = "Path_G08_G10"
  L5_2[3] = "Path_G08_G10"
  L5_2[4] = "Path_G08_G10"
  tG08 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_B09_C09"
  L5_2[2] = "Path_B09_B10"
  L5_2[3] = "Path_B09_B10"
  L5_2[4] = "Path_B09_B10"
  tB09 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_C09_D09"
  L5_2[2] = "Path_C09_B09"
  L5_2[3] = "Path_C09_C10"
  L5_2[4] = "Path_C09_C10"
  L5_2[5] = "Path_C09_C10"
  tC09 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_D09_E09"
  L5_2[2] = "Path_D09_C09"
  L5_2[3] = "Path_D09_D10"
  L5_2[4] = "Path_D09_D10"
  L5_2[5] = "Path_D09_D10"
  tD09 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_E09_D09"
  L5_2[2] = "Path_E09_E10"
  L5_2[3] = "Path_E09_E10"
  L5_2[4] = "Path_E09_E10"
  tE09 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_B10_C10"
  L5_2[2] = "Path_B10_EX1"
  L5_2[3] = "Path_B10_EX1"
  L5_2[4] = "Path_B10_EX1"
  tB10 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_C10_D10"
  L5_2[2] = "Path_C10_B10"
  L5_2[3] = "Path_C10_E11"
  L5_2[4] = "Path_C10_E11"
  L5_2[5] = "Path_C10_E11"
  tC10 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_D10_E10"
  L5_2[2] = "Path_D10_C10"
  tD10 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_E10_F10"
  L5_2[2] = "Path_E10_D10"
  L5_2[3] = "Path_E10_E11"
  L5_2[4] = "Path_E10_E11"
  L5_2[5] = "Path_E10_E11"
  tE10 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_F10_G10"
  L5_2[2] = "Path_F10_E10"
  tF10 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_G10_F10"
  L5_2[2] = "Path_G10_G11"
  L5_2[3] = "Path_G10_G11"
  L5_2[4] = "Path_G10_G11"
  tG10 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_E11_G11"
  L5_2[2] = "Path_E11_EX2"
  L5_2[3] = "Path_E11_EX2"
  L5_2[4] = "Path_E11_EX2"
  L5_2[5] = "Path_E11_EX2"
  tE11 = L5_2
  L5_2 = {}
  L5_2[1] = "Path_G11_E10"
  L5_2[2] = "Path_G11_EX3"
  L5_2[3] = "Path_G11_EX3"
  L5_2[4] = "Path_G11_EX3"
  L5_2[5] = "Path_G11_EX3"
  tG11 = L5_2
  L5_2 = {}
  L5_2[1] = "Intersection_x01"
  L5_2[2] = "Intersection_x02"
  L5_2[3] = "Intersection_x03"
  L5_2[4] = "Intersection_x04"
  L5_2[5] = "Intersection_x05"
  L5_2[6] = "Intersection_x06"
  L5_2[7] = "Intersection_x07"
  L5_2[8] = "Intersection_x08"
  L5_2[9] = "Intersection_x09"
  L5_2[10] = "Intersection_x10"
  L5_2[11] = "Intersection_x11"
  L5_2[12] = "Intersection_x12"
  L5_2[13] = "Intersection_x13"
  L5_2[14] = "Intersection_x14"
  L5_2[15] = "Intersection_B01"
  L5_2[16] = "Intersection_D01"
  L5_2[17] = "Intersection_E01"
  L5_2[18] = "Intersection_F01"
  L5_2[19] = "Intersection_B02"
  L5_2[20] = "Intersection_D02"
  L5_2[21] = "Intersection_E02"
  L5_2[22] = "Intersection_F02"
  L5_2[23] = "Intersection_G02"
  L5_2[24] = "Intersection_B03"
  L5_2[25] = "Intersection_G03"
  L5_2[26] = "Intersection_B04"
  L5_2[27] = "Intersection_D04"
  L5_2[28] = "Intersection_A05"
  L5_2[29] = "Intersection_D05"
  L5_2[30] = "Intersection_E05"
  L5_2[31] = "Intersection_G05"
  L5_2[32] = "Intersection_H05"
  L5_2[33] = "Intersection_B06"
  L5_2[34] = "Intersection_D06"
  L5_2[35] = "Intersection_B07"
  L5_2[36] = "Intersection_G07"
  L5_2[37] = "Intersection_B08"
  L5_2[38] = "Intersection_C08"
  L5_2[39] = "Intersection_D08"
  L5_2[40] = "Intersection_E08"
  L5_2[41] = "Intersection_F08"
  L5_2[42] = "Intersection_G08"
  L5_2[43] = "Intersection_B09"
  L5_2[44] = "Intersection_C09"
  L5_2[45] = "Intersection_D09"
  L5_2[46] = "Intersection_E09"
  L5_2[47] = "Intersection_B10"
  L5_2[48] = "Intersection_C10"
  L5_2[49] = "Intersection_D10"
  L5_2[50] = "Intersection_E10"
  L5_2[51] = "Intersection_F10"
  L5_2[52] = "Intersection_G10"
  L5_2[53] = "Intersection_E11"
  L5_2[54] = "Intersection_G11"
  tIntersection = L5_2
  L5_2 = {}
  L5_2[1] = "Intersection_B04"
  L5_2[2] = "Intersection_D04"
  L5_2[3] = "Intersection_D05"
  L5_2[4] = "Intersection_E05"
  L5_2[5] = "Intersection_G05"
  L5_2[6] = "Intersection_B06"
  L5_2[7] = "Intersection_D06"
  L5_2[8] = "Intersection_B07"
  L5_2[9] = "Intersection_G07"
  L5_2[10] = "Intersection_B08"
  L5_2[11] = "Intersection_C08"
  L5_2[12] = "Intersection_D08"
  L5_2[13] = "Intersection_E08"
  L5_2[14] = "Intersection_F08"
  L5_2[15] = "Intersection_G08"
  L5_2[16] = "Intersection_C09"
  L5_2[17] = "Intersection_D09"
  L5_2[18] = "Intersection_E09"
  L5_2[19] = "Intersection_C10"
  L5_2[20] = "Intersection_D10"
  L5_2[21] = "Intersection_E10"
  L5_2[22] = "Intersection_F10"
  L5_2[23] = "Intersection_G10"
  L5_2[24] = "Intersection_E11"
  L5_2[25] = "Intersection_G11"
  tPlayerNodes = L5_2
  L5_2 = {}
  L11_2 = tX06
  L12_2 = tX07
  L13_2 = tX08
  L14_2 = tX09
  L15_2 = tX10
  L16_2 = tX11
  L17_2 = tX12
  L18_2 = tX13
  L19_2 = tX14
  L20_2 = tB01
  L21_2 = tD01
  L22_2 = tE01
  L23_2 = tF01
  L24_2 = tB02
  L25_2 = tD02
  L26_2 = tE02
  L27_2 = tF02
  L28_2 = tG02
  L29_2 = tB03
  L30_2 = tG03
  L31_2 = tB04
  L32_2 = tD04
  L33_2 = tA05
  L34_2 = tD05
  L35_2 = tE05
  L36_2 = tG05
  L37_2 = tH05
  L38_2 = tB06
  L39_2 = tD06
  L40_2 = tB07
  L41_2 = tG07
  L42_2 = tB08
  L43_2 = tC08
  L44_2 = tD08
  L45_2 = tE08
  L46_2 = tF08
  L47_2 = tG08
  L48_2 = tB09
  L49_2 = tC09
  L50_2 = tD09
  L51_2 = tE09
  L52_2 = tB10
  L53_2 = tC10
  L54_2 = tD10
  L55_2 = tE10
  L5_2[1] = tX01
  L5_2[2] = tX02
  L5_2[3] = tX03
  L5_2[4] = tX04
  L5_2[5] = tX05
  L5_2[6] = L11_2
  L5_2[7] = L12_2
  L5_2[8] = L13_2
  L5_2[9] = L14_2
  L5_2[10] = L15_2
  L5_2[11] = L16_2
  L5_2[12] = L17_2
  L5_2[13] = L18_2
  L5_2[14] = L19_2
  L5_2[15] = L20_2
  L5_2[16] = L21_2
  L5_2[17] = L22_2
  L5_2[18] = L23_2
  L5_2[19] = L24_2
  L5_2[20] = L25_2
  L5_2[21] = L26_2
  L5_2[22] = L27_2
  L5_2[23] = L28_2
  L5_2[24] = L29_2
  L5_2[25] = L30_2
  L5_2[26] = L31_2
  L5_2[27] = L32_2
  L5_2[28] = L33_2
  L5_2[29] = L34_2
  L5_2[30] = L35_2
  L5_2[31] = L36_2
  L5_2[32] = L37_2
  L5_2[33] = L38_2
  L5_2[34] = L39_2
  L5_2[35] = L40_2
  L5_2[36] = L41_2
  L5_2[37] = L42_2
  L5_2[38] = L43_2
  L5_2[39] = L44_2
  L5_2[40] = L45_2
  L5_2[41] = L46_2
  L5_2[42] = L47_2
  L5_2[43] = L48_2
  L5_2[44] = L49_2
  L5_2[45] = L50_2
  L5_2[46] = L51_2
  L5_2[47] = L52_2
  L5_2[48] = L53_2
  L5_2[49] = L54_2
  L5_2[50] = L55_2
  L5_2[51] = tF10
  L5_2[52] = tG10
  L5_2[53] = tE11
  L5_2[54] = tG11
  tNodes = L5_2
  L6_2 = A0_2
  L5_2 = A0_2["_CreateEvent"]
  L7_2 = Event["ObjectDeath"]
  L8_2 = {}
  L9_2 = Pg.GetGuidByName
  L10_2 = "DLCCon003_overpass_a"
  L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2, L53_2, L54_2, L55_2 = L9_2(L10_2)
  L8_2[1] = L9_2
  L8_2[2] = L10_2
  L8_2[3] = L11_2
  L8_2[4] = L12_2
  L8_2[5] = L13_2
  L8_2[6] = L14_2
  L8_2[7] = L15_2
  L8_2[8] = L16_2
  L8_2[9] = L17_2
  L8_2[10] = L18_2
  L8_2[11] = L19_2
  L8_2[12] = L20_2
  L8_2[13] = L21_2
  L8_2[14] = L22_2
  L8_2[15] = L23_2
  L8_2[16] = L24_2
  L8_2[17] = L25_2
  L8_2[18] = L26_2
  L8_2[19] = L27_2
  L8_2[20] = L28_2
  L8_2[21] = L29_2
  L8_2[22] = L30_2
  L8_2[23] = L31_2
  L8_2[24] = L32_2
  L8_2[25] = L33_2
  L8_2[26] = L34_2
  L8_2[27] = L35_2
  L8_2[28] = L36_2
  L8_2[29] = L37_2
  L8_2[30] = L38_2
  L8_2[31] = L39_2
  L8_2[32] = L40_2
  L8_2[33] = L41_2
  L8_2[34] = L42_2
  L8_2[35] = L43_2
  L8_2[36] = L44_2
  L8_2[37] = L45_2
  L8_2[38] = L46_2
  L8_2[39] = L47_2
  L8_2[40] = L48_2
  L8_2[41] = L49_2
  L8_2[42] = L50_2
  L8_2[43] = L51_2
  L8_2[44] = L52_2
  L8_2[45] = L53_2
  L8_2[46] = L54_2
  L8_2[47] = L55_2
  
  function L9_2(A0_3)
    local L1_3, L2_3
    L1_3 = {}
    L1_3[1] = "Path_D06_B06"
    tD06 = L1_3
  end
  
  L10_2 = {}
  L11_2 = A0_2
  L10_2[1] = L11_2
  eOverpassA = L5_2(L6_2, L7_2, L8_2, L9_2, L10_2)
  L6_2 = A0_2
  L5_2 = A0_2["_CreateEvent"]
  L7_2 = Event["ObjectDeath"]
  L8_2 = {}
  L9_2 = Pg.GetGuidByName
  L10_2 = "DLCCon003_overpass_b"
  L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2, L53_2, L54_2, L55_2 = L9_2(L10_2)
  L8_2[1] = L9_2
  L8_2[2] = L10_2
  L8_2[3] = L11_2
  L8_2[4] = L12_2
  L8_2[5] = L13_2
  L8_2[6] = L14_2
  L8_2[7] = L15_2
  L8_2[8] = L16_2
  L8_2[9] = L17_2
  L8_2[10] = L18_2
  L8_2[11] = L19_2
  L8_2[12] = L20_2
  L8_2[13] = L21_2
  L8_2[14] = L22_2
  L8_2[15] = L23_2
  L8_2[16] = L24_2
  L8_2[17] = L25_2
  L8_2[18] = L26_2
  L8_2[19] = L27_2
  L8_2[20] = L28_2
  L8_2[21] = L29_2
  L8_2[22] = L30_2
  L8_2[23] = L31_2
  L8_2[24] = L32_2
  L8_2[25] = L33_2
  L8_2[26] = L34_2
  L8_2[27] = L35_2
  L8_2[28] = L36_2
  L8_2[29] = L37_2
  L8_2[30] = L38_2
  L8_2[31] = L39_2
  L8_2[32] = L40_2
  L8_2[33] = L41_2
  L8_2[34] = L42_2
  L8_2[35] = L43_2
  L8_2[36] = L44_2
  L8_2[37] = L45_2
  L8_2[38] = L46_2
  L8_2[39] = L47_2
  L8_2[40] = L48_2
  L8_2[41] = L49_2
  L8_2[42] = L50_2
  L8_2[43] = L51_2
  L8_2[44] = L52_2
  L8_2[45] = L53_2
  L8_2[46] = L54_2
  L8_2[47] = L55_2
  
  function L9_2(A0_3)
    local L1_3, L2_3
    L1_3 = {}
    L1_3[1] = "Path_E05_D05"
    tE05 = L1_3
  end
  
  L10_2 = {}
  L10_2[1] = A0_2
  eOverpassB = L5_2(L6_2, L7_2, L8_2, L9_2, L10_2)
  L5_2 = nRound
  L6_2 = 1
  if L5_2 == L6_2 then
    L5_2 = {}
    L6_2 = "PLZ45 (DLC) (LongHib) (Prototype)"
    L7_2 = "ZTZ63a (DLC) (LongHib) (Prototype)"
    L8_2 = "ZTZ98 (DLC) (LongHib) (Prototype)"
    L5_2[1] = L6_2
    L5_2[2] = L7_2
    L5_2[3] = L8_2
    tSpawnType = L5_2
  else
    L5_2 = nRound
    L6_2 = 2
    if L5_2 == L6_2 then
      L5_2 = {}
      L6_2 = "ZTZ63a (DLC) (LongHib) (Prototype)"
      L7_2 = "ZTZ63a (DLC) (LongHib) (Prototype)"
      L8_2 = "ZTZ98 (DLC) (LongHib) (Prototype)"
      L5_2[1] = L6_2
      L5_2[2] = L7_2
      L5_2[3] = L8_2
      tSpawnType = L5_2
    else
      L5_2 = nRound
      L6_2 = 3
      if L5_2 == L6_2 then
        L5_2 = {}
        L6_2 = "ZTZ63a (DLC) (LongHib) (Prototype)"
        L7_2 = "ZTZ98 (DLC) (LongHib) (Prototype)"
        L8_2 = "ZTZ98 (DLC) (LongHib) (Prototype)"
        L5_2[1] = L6_2
        L5_2[2] = L7_2
        L5_2[3] = L8_2
        tSpawnType = L5_2
      end
    end
  end
  nPlyrLoc = 2
  MrxTaskContract["Activated"](A0_2)
  A0_2["_SetCancelMessage"](A0_2, "")
  L5_2 = Hud["ResourceCounter"]
  L6_2 = L5_2
  L5_2 = L5_2["SetSuppressed"]
  L7_2 = {}
  L7_2["bSuppressCash"] = true
  L7_2["bSuppressFuel"] = true
  L5_2(L6_2, L7_2)
  MrxPmc["AddCashQty"](-MrxPmc["GetCashQty"](), nil, nil, false)
  Player["SetFuel"](0)
  L5_2 = Hud["ResourceCounter"]
  L6_2 = L5_2
  L5_2 = L5_2["SetSuppressed"]
  L7_2 = {}
  L7_2["bSuppressCash"] = false
  L7_2["bSuppressFuel"] = false
  L5_2(L6_2, L7_2)
  Ai["SetRelation"](GetGuidByName("China"), GetGuidByName("PMC"), -100)
  Ai["SetRelation"](GetGuidByName("Allied"), GetGuidByName("PMC"), 100)
  Player["SetVehicleDisguise"](false)
  L5_2 = Hud["ObjectiveTray"]
  L6_2 = L5_2
  L5_2 = L5_2["SetSlotToText"]
  L7_2 = {}
  L7_2["nSlot"] = 1
  L7_2["sText"] = ("[DLCCon003.Display.escapedTanks] " .. nGoal .. "/" .. nFailCon)
  L5_2(L6_2, L7_2)
  L5_2 = Hud["ResourceCounter"]
  L6_2 = L5_2
  L5_2 = L5_2["Show"]
  L7_2 = {}
  L7_2["nDuration"] = -1
  L5_2(L6_2, L7_2)
  L5_2 = Hud["ResourceCounter"]
  L6_2 = L5_2
  L5_2 = L5_2["SetSuppressed"]
  L7_2 = {}
  L7_2["bSuppressCash"] = false
  L7_2["bSuppressFuel"] = true
  L5_2(L6_2, L7_2)
  L5_2 = 2
  if L4_2 == L5_2 then
    A0_2["MultiplayerOn"](A0_2, true)
  else
    A0_2["MultiplayerOff"](A0_2)
  end
  L5_2 = Vehicle["GetFromRider"]
  L7_2 = "GetPrimaryCharacter"
  L6_2 = Player[L7_2]
  L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2, L53_2, L54_2, L55_2 = L6_2()
  uPlayerOneTank = L5_2(L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2, L53_2, L54_2, L55_2)
  L5_2 = uPlayerOneTank
  if L5_2 then
    A0_2["SetupPlayerTank"](A0_2, uPlayerOneTank)
    A0_2["TankLife"](A0_2, L2_2)
  end
  L5_2 = Vehicle["GetFromRider"]
  L7_2 = "GetSecondaryCharacter"
  L6_2 = Player[L7_2]
  L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2, L53_2, L54_2, L55_2 = L6_2()
  uPlayerTwoTank = L5_2(L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2, L52_2, L53_2, L54_2, L55_2)
  L5_2 = uPlayerTwoTank
  if L5_2 then
    A0_2["SetupPlayerTank"](A0_2, uPlayerTwoTank)
    A0_2["TankLife"](A0_2, L3_2)
  end
  L6_2 = A0_2
  L5_2 = A0_2["_CreateEvent"]
  L7_2 = Event["TimerRelative"]
  L8_2 = {}
  L8_2[1] = 10
  
  function L9_2(A0_3)
    local L1_3, L2_3
    A0_3.TestSpawn(A0_3)
  end
  
  L10_2 = {}
  L10_2[1] = A0_2
  eSpawnInit = L5_2(L6_2, L7_2, L8_2, L9_2, L10_2)
  L5_2 = ipairs
  L6_2 = tExitLocs
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = Pg.GetGuidByName(L9_2)
    L11_2 = "Blip_" .. tostring(L9_2)
    L12_2 = "PDA_Blip_" .. tostring(L9_2)
    L13_2 = Minimap
    L13_2["AddObjectiveWithGuid"](L13_2, L11_2, L10_2, 0, 0, 0, 255, 200, 0, 10, 10, "HUD_objective_defend", true, nil, nil, 5)
    table["insert"](tExitLocMarkers, Marker["AddBlip"](L10_2, "HUD_objective_defend", 32, 255, 200, 0, 255, 3.5))
    table["insert"](tExitLocMarkers, Marker["AddDisc"](L10_2, 10, 255, 200, 0, 0))
    L15_2 = Pda["Map"]
    L16_2 = L15_2
    L15_2 = L15_2["AddBlip"]
    L17_2 = {}
    L17_2["sName"] = L12_2
    L17_2["uGuid"] = L10_2
    L17_2["sTexture"] = "icon_defend_1_mc"
    L17_2["nSortOrder"] = 5
    L17_2["sLabel"] = "[DLCCon003.Objectives.003]"
    L17_2["sMission"] = "DlcCon003"
    L15_2(L16_2, L17_2)
  end
  L5_2 = ipairs
  L6_2 = tBridges
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = A0_2["_CreateEvent"]
    L12_2 = Event["ObjectDeath"]
    L13_2 = {}
    L13_2[1] = L9_2
    L15_2 = {}
    L15_2[1] = A0_2
    L10_2(A0_2, L12_2, L13_2, BridgeDeath, L15_2)
  end
  A0_2["InitalizeObj"](A0_2)
  A0_2["TracerFireWakeUpA"](A0_2)
  A0_2["TracerFireWakeUpB"](A0_2)
  A0_2["TracerFireExpA"](A0_2)
  A0_2["TracerFireExpB"](A0_2)
  A0_2["DistExplosion"](A0_2)
  A0_2["SetupMusic"](A0_2)
end

Activated = L0_1

function L0_1()
  local L0_2, L1_2
  MrxMusic.PlaySpecialMusic("Dlc_mu_tankbattle")
end

SetupMusic = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  L2_2 = A0_2
  L1_2 = A0_2._CreateEvent
  L3_2 = Event.TimerRelative
  L4_2 = {}
  L4_2[1] = 2
  
  function L5_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3
    L1_3 = {}
    L1_3[1] = "Fiona-In-Mission-Contract-Dlc03-01"
    L1_3[2] = 2
    L1_3[3] = "Fiona-In-Mission-Contract-Dlc03-02"
    L1_3[4] = 3
    L1_3[5] = "Fiona-In-Mission-Contract-Dlc03-03"
    MrxVoSequence.Start(L1_3)
  end
  
  L1_2(L2_2, L3_2, L4_2, L5_2)
end

MissionStartVO = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = "[DLCCon003.Display.replacementTanks]"
  L3_2 = {}
  L3_2[1] = "Fiona-In-Mission-Contract-Dlc03-17"
  L3_2[2] = "Fiona-In-Mission-Contract-Dlc03-18"
  L3_2[3] = "Fiona-In-Mission-Contract-Dlc03-19"
  L4_2 = 1
  L5_2 = nTankHealth - nCurTankHealth
  L6_2 = 1
  for L7_2 = L4_2, L5_2, L6_2 do
    L2_2 = L2_2 .. " [green]X"
  end
  L4_2 = 1
  L5_2 = nCurTankHealth
  L6_2 = 1
  for L7_2 = L4_2, L5_2, L6_2 do
    L2_2 = L2_2 .. " [red]X"
  end
  L4_2 = Hud.ObjectiveTray
  L6_2 = {}
  L6_2.vPlayer = A1_2
  L6_2.nSlot = 2
  L6_2.sText = L2_2
  L6_2.bDontNetSync = true
  L4_2.SetSlotToText(L4_2, L6_2)
  L4_2 = nPlayerTanks
  if L4_2 < 3 then
    L4_2 = nPlayerTanks
    if 0 < L4_2 then
      L5_2 = {}
      L5_2[1] = L3_2[nPlayerTanks]
      MrxVoSequence.Start(L5_2)
    end
  end
  nPlayerTanks = (nPlayerTanks + 1)
end

TankLife = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = {}
  L7_2 = 6
  L1_2[1] = 1
  L1_2[2] = 2
  L1_2[3] = 3
  L1_2[4] = 4
  L1_2[5] = 5
  L1_2[6] = L7_2
  tShellTime = L1_2
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tShellTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, RandomShellingCall, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tShellTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, RandomShellingCall, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tShellTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L6_2[1] = A0_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, RandomShellingCall, L6_2)
end

RandomShellingWakeUp = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = {}
  L2_2 = nPlyrLoc
  if L2_2 == 1 then
    L2_2 = {}
    L2_2[1] = "loc_shell_a1"
    L2_2[2] = "loc_shell_a2"
    L2_2[3] = "loc_shell_a3"
    L2_2[4] = "loc_shell_a4"
    L1_2 = L2_2
  else
    L2_2 = nPlyrLoc
    if L2_2 == 2 then
      L2_2 = {}
      L2_2[1] = "loc_shell_b1"
      L2_2[2] = "loc_shell_b2"
      L2_2[3] = "loc_shell_b3"
      L2_2[4] = "loc_shell_b4"
      L1_2 = L2_2
    else
      L2_2 = nPlyrLoc
      if L2_2 == 3 then
        L2_2 = {}
        L2_2[1] = "loc_shell_c1"
        L2_2[2] = "loc_shell_c2"
        L2_2[3] = "loc_shell_c3"
        L2_2[4] = "loc_shell_c4"
        L1_2 = L2_2
      else
        L2_2 = nPlyrLoc
        if L2_2 == 4 then
          L2_2 = {}
          L2_2[1] = "loc_shell_d1"
          L2_2[2] = "loc_shell_d2"
          L2_2[3] = "loc_shell_d3"
          L2_2[4] = "loc_shell_d4"
          L1_2 = L2_2
        else
          L2_2 = nPlyrLoc
          if L2_2 == 5 then
            L2_2 = {}
            L2_2[1] = "loc_shell_e1"
            L2_2[2] = "loc_shell_e2"
            L2_2[3] = "loc_shell_e3"
            L2_2[4] = "loc_shell_e4"
            L1_2 = L2_2
          else
            L2_2 = nPlyrLoc
            if L2_2 == 6 then
              L2_2 = {}
              L2_2[1] = "loc_shell_f1"
              L2_2[2] = "loc_shell_f2"
              L2_2[3] = "loc_shell_f3"
              L2_2[4] = "loc_shell_f4"
              L1_2 = L2_2
            else
              L2_2 = nPlyrLoc
              if L2_2 == 7 then
                L2_2 = {}
                L2_2[1] = "loc_shell_d1"
                L2_2[2] = "loc_shell_d2"
                L2_2[3] = "loc_shell_d3"
                L2_2[4] = "loc_shell_d4"
                L1_2 = L2_2
              else
                L2_2 = nPlyrLoc
                if L2_2 == 8 then
                  L2_2 = {}
                  L2_2[1] = "loc_shell_e1"
                  L2_2[2] = "loc_shell_e2"
                  L2_2[3] = "loc_shell_e3"
                  L2_2[4] = "loc_shell_e4"
                  L1_2 = L2_2
                else
                  L2_2 = nPlyrLoc
                  if L2_2 == 9 then
                    L2_2 = {}
                    L2_2[1] = "loc_shell_f1"
                    L2_2[2] = "loc_shell_f2"
                    L2_2[3] = "loc_shell_f3"
                    L2_2[4] = "loc_shell_f4"
                    L1_2 = L2_2
                  end
                end
              end
            end
          end
        end
      end
    end
  end
  sRandomShell = MrxUtil.GetRandomTableElement(L1_2)
  RandomShelling(A0_2, sRandomShell)
end

RandomShellingCall = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2
  L2_2 = "Rocket Artillery Projectile"
  L3_2 = 0
  L4_2 = 1
  L6_2 = 15
  L7_2 = Pg.GetGuidByName(A1_2)
  L8_2 = Object.GetPosition
  L9_2 = L7_2
  L8_2, L9_2, L10_2 = L8_2(L9_2)
  L11_2 = Object.GetPosition
  L12_2 = Player.GetPrimaryCharacter
  L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2 = L12_2()
  L11_2, L12_2, L13_2 = L11_2(L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2)
  L14_2 = L8_2 - ((math.randf() * 10) - (math.randf() * 10))
  L18_2 = Math.Normalize
  L19_2 = (L14_2 - L11_2)
  L20_2 = (L9_2 - L12_2)
  L18_2, L19_2, L20_2 = L18_2(L19_2, L20_2, (L10_2 - L13_2))
  L17_2 = L20_2
  L16_2 = L19_2
  L15_2 = L18_2
  L18_2 = -(L10_2 - L13_2)
  L21_2 = Math.Normalize
  L22_2 = L18_2
  L23_2 = 0
  L21_2, L22_2, L23_2 = L21_2(L22_2, L23_2, (L14_2 - L11_2))
  L20_2 = L23_2
  L19_2 = L22_2
  L18_2 = L21_2
  L21_2 = 1
  L22_2 = 1
  L23_2 = 1
  for L24_2 = L21_2, L22_2, L23_2 do
    L25_2 = L3_2
    L26_2 = L4_2
    L27_2 = -(((math.randf() * L25_2) - (math.randf() * L25_2)) + ((2.5 - L24_2) * L25_2))
    L28_2 = -((math.randf() * L26_2) - (math.randf() * L26_2))
    L29_2 = {}
    L29_2.sAmmo = L2_2
    L29_2.nTargetX = ((L14_2 + (L18_2 * L27_2)) + (L15_2 * L28_2))
    L29_2.nTargetY = (L9_2 + 250)
    L29_2.nTargetZ = ((L10_2 + (L20_2 * L27_2)) + (L17_2 * L28_2))
    L30_2 = uStrikeLoc
    if L30_2 then
      L30_2 = Pg.GetGuidByName(("loc_Rockets_" .. uEncounter .. "_" .. L24_2))
      if L30_2 then
        L31_2 = Object.GetPosition
        L32_2 = L30_2
        L31_2, L32_2, L33_2 = L31_2(L32_2)
        L34_2 = {}
        L34_2.sAmmo = L2_2
        L34_2.nTargetX = L31_2
        L34_2.nTargetY = (L32_2 + 250)
        L34_2.nTargetZ = L33_2
      else
        L29_2 = nil
      end
    end
    L30_2 = Pg.GetGuidByName("China")
    if L29_2 then
      L34_2 = {}
      L34_2[1] = (2 + (L24_2 * (L6_2 / 22)))
      L36_2 = {}
      L36_2[1] = L29_2
      L36_2[2] = L30_2
      A0_2._CreateEvent(A0_2, Event.TimerRelative, L34_2, TriggerFallingMissile, L36_2)
    end
  end
end

RandomShelling = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L2_2 = Airstrike.SpawnOrdnance(A0_2.sAmmo, A0_2.nTargetX, A0_2.nTargetY, A0_2.nTargetZ, 0, -100, 0, "impact", 1, A1_2)
end

TriggerFallingMissile = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  _OutsideBoundary(A0_2, Pg.GetGuidByName("rgn_checkspawn_a"), 1)
  _OutsideBoundary(A0_2, Pg.GetGuidByName("rgn_checkspawn_b"), 2)
  _OutsideBoundary(A0_2, Pg.GetGuidByName("rgn_checkspawn_c"), 3)
  _OutsideBoundary(A0_2, Pg.GetGuidByName("rgn_checkspawn_d"), 4)
  _OutsideBoundary(A0_2, Pg.GetGuidByName("rgn_checkspawn_e"), 5)
  _OutsideBoundary(A0_2, Pg.GetGuidByName("rgn_checkspawn_f"), 6)
  _OutsideBoundary(A0_2, Pg.GetGuidByName("rgn_checkspawn_g"), 7)
  _OutsideBoundary(A0_2, Pg.GetGuidByName("rgn_checkspawn_h"), 8)
  _OutsideBoundary(A0_2, Pg.GetGuidByName("rgn_checkspawn_i"), 9)
end

BoundaryTriggers = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  nPlyrLoc = A2_2
  L6_2 = {}
  L6_2[1] = Player.GetAnyCharacter()
  L6_2[2] = A1_2
  L6_2[3] = "exit"
  L6_2[4] = false
  L8_2 = {}
  L8_2[1] = A0_2
  L8_2[2] = A1_2
  L8_2[3] = A2_2
  A0_2._CreateEvent(A0_2, Event.Boundary, L6_2, _OutsideBoundary, L8_2)
end

_InsideBoundary = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L6_2 = {}
  L6_2[1] = Player.GetAnyCharacter()
  L6_2[2] = A1_2
  L6_2[3] = "enter"
  L6_2[4] = false
  L8_2 = {}
  L8_2[1] = A0_2
  L8_2[2] = A1_2
  L8_2[3] = A2_2
  A0_2._CreateEvent(A0_2, Event.Boundary, L6_2, _InsideBoundary, L8_2)
end

_OutsideBoundary = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  L1_2 = {}
  L2_2 = Pg.GetGuidByName("rgn_checkspawn_a")
  L3_2 = Pg.GetGuidByName("rgn_checkspawn_b")
  L4_2 = Pg.GetGuidByName("rgn_checkspawn_c")
  L5_2 = Pg.GetGuidByName("rgn_checkspawn_d")
  L6_2 = Pg.GetGuidByName("rgn_checkspawn_e")
  L7_2 = Pg.GetGuidByName("rgn_checkspawn_f")
  L8_2 = Pg.GetGuidByName("rgn_checkspawn_g")
  L9_2 = Pg.GetGuidByName("rgn_checkspawn_h")
  L10_2 = Pg.GetGuidByName
  L11_2 = "rgn_checkspawn_i"
  L10_2, L11_2, L12_2, L13_2, L14_2, L15_2 = L10_2(L11_2)
  L1_2[1] = L2_2
  L1_2[2] = L3_2
  L1_2[3] = L4_2
  L1_2[4] = L5_2
  L1_2[5] = L6_2
  L1_2[6] = L7_2
  L1_2[7] = L8_2
  L1_2[8] = L9_2
  L1_2[9] = L10_2
  L1_2[10] = L11_2
  L1_2[11] = L12_2
  L1_2[12] = L13_2
  L1_2[13] = L14_2
  L1_2[14] = L15_2
  L2_2 = {}
  L5_2 = 3
  L8_2 = 6
  L2_2[1] = 1
  L2_2[2] = 2
  L2_2[3] = L5_2
  L2_2[4] = 4
  L2_2[5] = 5
  L2_2[6] = L8_2
  L2_2[7] = 7
  L2_2[8] = 8
  L2_2[9] = 9
  L3_2 = Object.GetPosition
  L4_2 = uPlayerOne
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  L6_2 = ipairs
  L7_2 = L1_2
  L6_2, L7_2, L8_2 = L6_2(L7_2)
  for L9_2, L10_2 in L6_2, L7_2, L8_2 do
    bLocated = Pg.IsPointInBoundary(L3_2, L4_2, L5_2, L10_2)
    L11_2 = bLocated
    if L11_2 then
      L11_2 = L2_2[L9_2]
      return L11_2
    end
  end
end

PlayerLocation = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  nPlyrLoc = A0_2.PlayerLocation(A0_2)
  L2_2 = nSpawn
  if L2_2 < 16 then
    sSpawnType = tSpawnType[1]
  else
    L2_2 = nSpawn
    if L2_2 < 36 then
      L2_2 = nSpawn
      if 15 < L2_2 then
        sSpawnType = tSpawnType[2]
    end
    else
      L2_2 = nSpawn
      if 35 < L2_2 then
        sSpawnType = tSpawnType[3]
      end
    end
  end
  L2_2 = nPlyrLoc
  if L2_2 == 1 then
    L2_2 = {}
    L2_2[1] = "spawna_1"
    L2_2[2] = "spawna_2"
    L2_2[3] = "spawna_3"
    L2_2[4] = "spawna_4"
    L2_2[5] = "spawnb_1"
    L2_2[6] = "spawnb_2"
    L2_2[7] = "spawnb_3"
    L2_2[8] = "spawnb_4"
    tSpawners = L2_2
  else
    L2_2 = nPlyrLoc
    if L2_2 == 2 then
      L2_2 = {}
      L2_2[1] = "spawna_1"
      L2_2[2] = "spawna_2"
      L2_2[3] = "spawna_3"
      L2_2[4] = "spawna_4"
      L2_2[5] = "spawnc_1"
      L2_2[6] = "spawnc_2"
      L2_2[7] = "spawnc_3"
      L2_2[8] = "spawnc_4"
      tSpawners = L2_2
    else
      L2_2 = nPlyrLoc
      if L2_2 == 3 then
        L2_2 = {}
        L2_2[1] = "spawnb_1"
        L2_2[2] = "spawnb_2"
        L2_2[3] = "spawnb_3"
        L2_2[4] = "spawnb_4"
        L2_2[5] = "spawnc_1"
        L2_2[6] = "spawnc_2"
        L2_2[7] = "spawnc_3"
        L2_2[8] = "spawnc_4"
        tSpawners = L2_2
      else
        L2_2 = nPlyrLoc
        if L2_2 == 4 then
          L2_2 = {}
          L2_2[1] = "spawnd_1"
          L2_2[2] = "spawnd_2"
          L2_2[3] = "spawnd_3"
          L2_2[4] = "spawnd_4"
          L2_2[5] = "spawne_1"
          L2_2[6] = "spawne_2"
          L2_2[7] = "spawne_3"
          L2_2[8] = "spawne_4"
          tSpawners = L2_2
        else
          L2_2 = nPlyrLoc
          if L2_2 == 5 then
            L2_2 = {}
            L2_2[1] = "spawnd_1"
            L2_2[2] = "spawnd_2"
            L2_2[3] = "spawnd_3"
            L2_2[4] = "spawnd_4"
            L2_2[5] = "spawnf_1"
            L2_2[6] = "spawnf_2"
            L2_2[7] = "spawnf_3"
            L2_2[8] = "spawnf_4"
            tSpawners = L2_2
          else
            L2_2 = nPlyrLoc
            if L2_2 == 6 then
              L2_2 = {}
              L2_2[1] = "spawne_1"
              L2_2[2] = "spawne_2"
              L2_2[3] = "spawne_3"
              L2_2[4] = "spawne_4"
              L2_2[5] = "spawnf_1"
              L2_2[6] = "spawnf_2"
              L2_2[7] = "spawnf_3"
              L2_2[8] = "spawnf_4"
              tSpawners = L2_2
            else
              L2_2 = nPlyrLoc
              if L2_2 == 7 then
                L2_2 = {}
                L2_2[1] = "spawna_1"
                L2_2[2] = "spawna_2"
                L2_2[3] = "spawna_3"
                L2_2[4] = "spawna_4"
                L2_2[5] = "spawnb_1"
                L2_2[6] = "spawnb_2"
                L2_2[7] = "spawnb_3"
                L2_2[8] = "spawnb_4"
                tSpawners = L2_2
              else
                L2_2 = nPlyrLoc
                if L2_2 == 8 then
                  L2_2 = {}
                  L2_2[1] = "spawna_1"
                  L2_2[2] = "spawna_2"
                  L2_2[3] = "spawna_3"
                  L2_2[4] = "spawna_4"
                  L2_2[5] = "spawnc_1"
                  L2_2[6] = "spawnc_2"
                  L2_2[7] = "spawnc_3"
                  L2_2[8] = "spawnc_4"
                  tSpawners = L2_2
                else
                  L2_2 = nPlyrLoc
                  if L2_2 == 9 then
                    L2_2 = {}
                    L2_2[1] = "spawnb_1"
                    L2_2[2] = "spawnb_2"
                    L2_2[3] = "spawnb_3"
                    L2_2[4] = "spawnb_4"
                    L2_2[5] = "spawnc_1"
                    L2_2[6] = "spawnc_2"
                    L2_2[7] = "spawnc_3"
                    L2_2[8] = "spawnc_4"
                    tSpawners = L2_2
                  end
                end
              end
            end
          end
        end
      end
    end
  end
  L2_2 = ipairs
  L3_2 = tSpawners
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = nLive
    if L7_2 < 13 then
      L7_2 = Pg.GetGuidByName(L6_2)
      L8_2 = math.randi(11) - 1
      L12_2 = {}
      L12_2[1] = L8_2
      L14_2 = {}
      L14_2[1] = A0_2
      L14_2[2] = sSpawnType
      L14_2[3] = L7_2
      A0_2._CreateEvent(A0_2, Event.TimerRelative, L12_2, SpawnEnemyTank, L14_2)
    end
  end
end

TestSpawn = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L3_2 = MrxUtil.SpawnObject(A1_2, A2_2)
  nLive = (nLive + 1)
  nSpawn = (nSpawn + 1)
  L4_2 = {}
  L4_2.tExitGoals = {}
  L5_2 = A0_2.tVehicleEvents
  L5_2[L3_2] = L4_2
  L8_2 = {}
  L8_2[1] = L3_2
  L8_2[2] = "awake"
  L10_2 = {}
  L10_2[1] = A0_2
  L10_2[2] = L3_2
  L4_2.eHibernate = A0_2._CreateEvent(A0_2, Event.ObjectHibernation, L8_2, SetupEnemyTank, L10_2)
  table.insert(tSpawned, L3_2)
end

SpawnEnemyTank = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2
  L2_2 = A0_2.tVehicleEvents[A1_2]
  L6_2 = {}
  L6_2[1] = A1_2
  L8_2 = {}
  L8_2[1] = A0_2
  L2_2.eDeath = A0_2._CreateEvent(A0_2, Event.ObjectDeath, L6_2, OnEnemyTankKilled, L8_2)
  L3_2 = Vehicle.GetDriver(A1_2)
  L6_2 = Event.ObjectInSeat
  L7_2 = {}
  L7_2[1] = L3_2
  L7_2[2] = A1_2
  L7_2[3] = "d"
  L7_2[4] = "x"
  L9_2 = {}
  L9_2[1] = A0_2
  L9_2[2] = A1_2
  L2_2.eHijack = A0_2._CreateEvent(A0_2, L6_2, L7_2, OnEnemyTankKilled, L9_2)
  L2_2.tRegions = {}
  L4_2 = ipairs
  L5_2 = tExitLocs
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    L9_2 = Pg.GetGuidByName(L8_2)
    A0_2.OnEnemyTankOutsideExitRegion(A0_2, A1_2, L8_2)
    L10_2 = L2_2.tExitGoals
    L14_2 = {}
    L14_2[1] = A1_2
    L14_2[2] = L9_2
    L14_2[3] = "<"
    L14_2[4] = 20
    L14_2[5] = false
    L14_2[6] = false
    L16_2 = {}
    L16_2[1] = A0_2
    L16_2[2] = A1_2
    L10_2[L9_2] = A0_2._CreateEvent(A0_2, Event.ObjectProximity, L14_2, OnEnemyTankEscaping, L16_2)
  end
  L4_2 = {}
  L4_2[1] = 7
  L4_2[2] = 7
  L4_2[3] = 7
  L4_2[4] = 8.5
  L4_2[5] = 10
  L10_2 = MrxUtil.GetRandomTableElement
  L11_2 = L4_2
  L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2 = L10_2(L11_2)
  A0_2.ChoosePath(A0_2, L3_2, nil, 0, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2)
end

SetupEnemyTank = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = Object.IsPlayerControlled(A1_2)
  if not L2_2 then
    Object.FadeOut(A1_2, 10, true)
  end
  nTankDeath = (nTankDeath + 1)
  L2_2 = "Alert_" .. tostring(A1_2)
  if L2_2 then
    L2_2 = Minimap
    L2_2.DeleteObjective(L2_2, ("Alert_" .. tostring(A1_2)))
  end
  nLive = (nLive - 1)
  L2_2 = nLive
  if L2_2 < 0 then
    nLive = 0
  end
  L2_2 = nLive
  if L2_2 < 3 then
    A0_2.TestSpawn(A0_2)
  end
  L2_2 = nLive
  if L2_2 < 5 then
    A0_2.RandomShellingWakeUp(A0_2)
  end
  L3_2 = nKillBonus[Object.GetParent(A1_2)]
  if L3_2 then
    A0_2.nTankMoney = (A0_2.nTankMoney + L3_2)
    MrxPmc.AddCashQty(L3_2, nil)
    L4_2 = Hud.ResourceCounter
    L6_2 = {}
    L6_2.nDuration = -1
    L4_2.Show(L4_2, L6_2)
    L4_2 = Hud.ResourceCounter
    L6_2 = {}
    L6_2.bSuppressCash = false
    L6_2.bSuppressFuel = true
    L4_2.SetSuppressed(L4_2, L6_2)
  else
  end
  L4_2 = nTankDeath
  L5_2 = nTBSpawn
  if L4_2 == L5_2 then
    A0_2.TankBusterSpawn(A0_2)
    nTBSpawn = (nTBSpawn + 25)
  end
  L4_2 = nTankDeath
  L5_2 = nRoundValue
  if L4_2 == L5_2 then
    nRound = (nRound + 1)
    nRoundValue = (nRoundValue + 20)
  end
  A0_2.RemoveEnemyTankFromRegionCounters(A0_2, A1_2)
  A0_2.CleanupEnemyTank(A0_2, A1_2)
end

OnEnemyTankKilled = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L2_2 = A0_2.tVehicleEvents[A1_2]
  L3_2 = pairs
  L4_2 = tExitLocs
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    L9_2 = L2_2.tRegions[Pg.GetGuidByName((L7_2 .. "_alert"))]
    if L9_2 then
      L9_2 = A0_2.tExitRegionCounters
      L9_2[L8_2] = (A0_2.tExitRegionCounters[L8_2] - 1)
      A0_2.UpdateExitRegion(A0_2, L7_2)
    end
  end
end

RemoveEnemyTankFromRegionCounters = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L3_2 = A0_2.tVehicleEvents[A1_2]
  L4_2 = Pg.GetGuidByName((A2_2 .. "_alert"))
  Event.Delete(L3_2.tExitGoals[L4_2])
  L5_2 = L3_2.tExitGoals
  L9_2 = {}
  L9_2[1] = A1_2
  L9_2[2] = L4_2
  L9_2[3] = "enter"
  L9_2[4] = false
  L11_2 = {}
  L11_2[1] = A0_2
  L11_2[2] = A1_2
  L11_2[3] = A2_2
  L5_2[L4_2] = A0_2._CreateEvent(A0_2, Event.Boundary, L9_2, OnEnemyTankTankInsideExitRegion, L11_2)
  L5_2 = L3_2.tRegions
  L5_2[L4_2] = nil
  L5_2 = A0_2.tExitRegionCounters[L4_2]
  if L5_2 then
    L5_2 = A0_2.tExitRegionCounters[L4_2]
    if 0 < L5_2 then
      L5_2 = A0_2.tExitRegionCounters
      L5_2[L4_2] = (A0_2.tExitRegionCounters[L4_2] - 1)
      A0_2.UpdateExitRegion(A0_2, A2_2)
    end
  else
    L5_2 = A0_2.tExitRegionCounters
    L5_2[L4_2] = 0
  end
end

OnEnemyTankOutsideExitRegion = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L3_2 = A0_2.tVehicleEvents[A1_2]
  L4_2 = Pg.GetGuidByName((A2_2 .. "_alert"))
  Event.Delete(L3_2.tExitGoals[L4_2])
  L5_2 = L3_2.tExitGoals
  L9_2 = {}
  L9_2[1] = A1_2
  L9_2[2] = L4_2
  L9_2[3] = "exit"
  L9_2[4] = false
  L11_2 = {}
  L11_2[1] = A0_2
  L11_2[2] = A1_2
  L11_2[3] = A2_2
  L5_2[L4_2] = A0_2._CreateEvent(A0_2, Event.Boundary, L9_2, OnEnemyTankOutsideExitRegion, L11_2)
  L5_2 = L3_2.tRegions
  L5_2[L4_2] = true
  L5_2 = A0_2.tExitRegionCounters
  L5_2[L4_2] = (A0_2.tExitRegionCounters[L4_2] + 1)
  A0_2.UpdateExitRegion(A0_2, A2_2, A1_2)
end

OnEnemyTankTankInsideExitRegion = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2
  L3_2 = Pg.GetGuidByName((A1_2 .. "_alert"))
  L4_2 = Pg.GetGuidByName(A1_2)
  L5_2 = "Blip_" .. A1_2
  L6_2 = A0_2.tExitRegionCounters[L3_2]
  if 0 < L6_2 then
    L6_2 = Hud.MessageBox
    L8_2 = {}
    L8_2.sMessage = "[DLCCon003.Terms.alert01]"
    L6_2.AddMessage(L6_2, L8_2)
    L6_2 = Hud.Radar
    L8_2 = {}
    L8_2.sName = L5_2
    L8_2.nDuration = 0
    L8_2.nMinAlpha = 0.1
    L8_2.nMaxAlpha = 0.8
    L8_2.nSpeed = 3
    L6_2.AnimateObjectiveAlpha(L6_2, L8_2)
    L7_2 = nil
    L8_2 = Object.GetPosition
    L9_2 = L4_2
    L8_2, L9_2, L10_2 = L8_2(L9_2)
    L14_2 = L10_2
    L11_2 = Pg.FastCollectTanks(L8_2, L9_2, L14_2, 75)
    L12_2 = pairs
    L13_2 = L11_2
    L12_2, L13_2, L14_2 = L12_2(L13_2)
    for L15_2, L16_2 in L12_2, L13_2, L14_2 do
      L17_2 = Object.HasLabel(L16_2, "china")
      if L17_2 then
        L17_2 = Object.IsPlayerControlled(L16_2)
        if not L17_2 then
          L7_2 = true
          break
        end
      end
    end
    if L7_2 then
      Marker.Pulse(L4_2, 255, 0, 0)
    elseif A2_2 then
      Marker.Pulse(L4_2, 255, 200, 0)
      L15_2 = {}
      L15_2[1] = A2_2
      L15_2[2] = L4_2
      L15_2[3] = "<"
      L15_2[4] = L6_2
      L15_2[5] = false
      L15_2[6] = false
      L17_2 = {}
      L17_2[1] = L4_2
      L17_2[2] = 255
      L17_2[3] = 0
      L17_2[4] = 0
      A0_2._CreateEvent(A0_2, Event.ObjectProximity, L15_2, Marker.Pulse, L17_2)
    else
      Marker.Pulse(L4_2, 255, 200, 0)
    end
  else
    L6_2 = Hud.Radar
    L8_2 = {}
    L8_2.sName = L5_2
    L6_2.UnanimateObjective(L6_2, L8_2)
    Marker.HaltPulse(L4_2)
  end
end

UpdateExitRegion = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L2_2 = A0_2.tVehicleEvents[A1_2]
  L4_2 = nEscPen[Object.GetParent(A1_2)]
  if not L4_2 then
    L4_2 = 1000000
  end
  Object.FadeOut(A1_2, 1.5, true)
  L8_2 = {}
  L8_2[1] = A1_2
  L10_2 = {}
  L10_2[1] = A0_2
  L10_2[2] = A1_2
  L10_2[3] = L4_2
  L2_2.eEscaping = A0_2._CreateEvent(A0_2, Event.ObjectDelete, L8_2, OnEnemyTankEscaped, L10_2)
end

OnEnemyTankEscaping = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L3_2 = Minimap
  L3_2.DeleteObjective(L3_2, ("Alert_" .. tostring(A1_2)))
  nGoal = (nGoal + 1)
  L3_2 = Hud.ObjectiveTray
  L5_2 = {}
  L5_2.nSlot = 1
  L5_2.sText = ("[DLCCon003.Display.escapedTanks]" .. nGoal .. "/" .. nFailCon)
  L3_2.SetSlotToText(L3_2, L5_2)
  MrxPmc.AddCashQty(-A2_2, nil, "[DLCCon003.Display.scoreEscaped]")
  A0_2.nEscapeMoney = (A0_2.nEscapeMoney + A2_2)
  L3_2 = Hud.ResourceCounter
  L5_2 = {}
  L5_2.nDuration = -1
  L3_2.Show(L3_2, L5_2)
  L3_2 = Hud.ResourceCounter
  L5_2 = {}
  L5_2.bSuppressCash = false
  L5_2.bSuppressFuel = true
  L3_2.SetSuppressed(L3_2, L5_2)
  L3_2 = Hud.MessageBox
  L5_2 = {}
  L5_2.sMessage = "[DLCCon003.Terms.escaped]"
  L3_2.AddMessage(L3_2, L5_2)
  L3_2 = nGoal
  L4_2 = nFailCon - 1
  if L3_2 < L4_2 then
    L3_2 = {}
    L3_2[1] = "Fiona-In-Mission-Contract-Dlc03-34"
    L3_2[2] = "Fiona-In-Mission-Contract-Dlc03-35"
    L3_2[3] = "Fiona-In-Mission-Contract-Dlc03-36"
    L3_2[4] = "Fiona-In-Mission-Contract-Dlc03-37"
    L3_2[5] = "Fiona-In-Mission-Contract-Dlc03-38"
    A0_2.NotifyPlayerVO(A0_2, L3_2, 0.5)
  else
    L3_2 = nGoal
    L4_2 = nFailCon - 1
    if L3_2 == L4_2 then
      L3_2 = {}
      L3_2[1] = "Fiona-In-Mission-Contract-Dlc03-38"
      A0_2.NotifyPlayerVO(A0_2, L3_2, 0.5)
    else
      A0_2.TanksEscaped(A0_2)
      return
    end
  end
  nLive = (nLive - 1)
  L3_2 = nLive
  if L3_2 < 0 then
    nLive = 0
  end
  L3_2 = nLive
  if L3_2 < 3 then
    A0_2.TestSpawn(A0_2)
  end
  A0_2.RemoveEnemyTankFromRegionCounters(A0_2, A1_2)
  A0_2.CleanupEnemyTank(A0_2, A1_2)
end

OnEnemyTankEscaped = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = A0_2.tVehicleEvents[A1_2]
  L2_2.tRegions = nil
  L3_2 = pairs
  L4_2 = L2_2.tExitGoals
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    Event.Delete(L7_2)
  end
  L2_2.tExitGoals = nil
  L3_2 = pairs
  L4_2 = L2_2
  L3_2, L4_2, L5_2 = L3_2(L4_2)
  for L6_2, L7_2 in L3_2, L4_2, L5_2 do
    Event.Delete(L7_2)
  end
  L3_2 = A0_2.tVehicleEvents
  L3_2[A1_2] = nil
end

CleanupEnemyTank = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2)
  local L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  L5_2 = A3_2 or nil
  if not A3_2 then
    L5_2 = 0
  end
  L6_2 = A0_2.FindIntersection
  if not L6_2 then
    return
  end
  L6_2 = A0_2.FindIntersection(A0_2, A1_2, (20 + L5_2))
  if not L6_2 then
    ChoosePath(A0_2, A1_2, sPath, (L5_2 + 10), A4_2)
  else
  end
  L9_2 = tCurNode[math.randi(table.getn(tCurNode))]
  if L9_2 == A2_2 then
    ChoosePath(A0_2, A1_2, L9_2, (L5_2 + 5), A4_2)
  else
    L10_2 = Pg.GetGuidByName(tCurNode[L8_2])
    L12_2 = {}
    L12_2.AIGuid = A1_2
    L12_2.Goal = "PathMove"
    L12_2.Target = L10_2
    L12_2.Haste = nAdv
    L12_2.Start = "First"
    L12_2.Mode = "Oneway"
    L12_2.Priority = 8.75
    L12_2.Callback = ChoosePath
    L13_2 = {}
    L13_2[1] = A0_2
    L13_2[2] = A1_2
    L13_2[3] = L9_2
    L13_2[4] = 0
    L13_2[5] = A4_2
    L12_2.CallbackData = L13_2
    Ai.Goal(L12_2)
  end
end

ChoosePath = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L4_2 = {}
  if A3_2 then
    tPotential = tPlayerNodes
  else
    tPotential = tIntersection
  end
  L5_2 = ipairs
  L6_2 = tPotential
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L11_2 = MrxUtil.GetDistanceBetween(A1_2, Pg.GetGuidByName(L9_2), true)
    if A2_2 > L11_2 then
      tCurNode = tNodes[L8_2]
      return L9_2
    end
  end
end

FindIntersection = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L2_2 = Hud.MessageBox
  L4_2 = {}
  L4_2.sMessage = "[DLCCon003.Terms.newPlayerTank_alt]"
  L2_2.AddMessage(L2_2, L4_2)
  L2_2 = A0_2.FindIntersection(A0_2, A1_2, 80, true)
  L3_2 = nil
  if L2_2 then
    L4_2 = Object.GetPosition
    L5_2 = Pg.GetGuidByName
    L6_2 = L2_2
    L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2 = L5_2(L6_2)
    L4_2, L5_2, L6_2 = L4_2(L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2)
    pZ = L6_2
    pY = L5_2
    pX = L4_2
  else
    L4_2 = Object.GetPosition
    L5_2 = Pg.GetGuidByName
    L6_2 = "DLCCon003_Player01_Tank_loc"
    L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2 = L5_2(L6_2)
    L4_2, L5_2, L6_2 = L4_2(L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2)
    pZ = L6_2
    pY = L5_2
    pX = L4_2
  end
  Object.FadeOut(A1_2, 3, true)
  L4_2 = Pg.Spawn("DLC_M1A3", pX, pY, pZ, 180, false, true)
  table.insert(tSpawned, L4_2)
  A0_2.SetupPlayerTank(A0_2, L4_2)
end

SpawnPlayerTank = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = A0_2._tEvents
  L6_2 = {}
  L6_2[1] = A1_2
  L8_2 = {}
  L8_2[1] = A0_2
  L2_2.ePlayerTankDeath = A0_2._CreateEvent(A0_2, Event.ObjectDeath, L6_2, OnPlayerTankDeath, L8_2)
  L2_2 = A0_2._tEvents
  L4_2 = A0_2
  L3_2 = A0_2._CreatePersistentEvent
  L5_2 = Event.ScriptEvent
  L6_2 = {}
  L7_2 = "RepairBayUsed"
  
  function L8_2(A0_3)
    local L1_3, L2_3, L3_3
    nRepairMoney = (nRepairMoney + A0_3[3])
    L1_3 = Object.HasLabel(A0_3[1], "Allied")
    if L1_3 then
      L1_3 = Object.HasLabel(A0_3[1], "Tank")
    end
    return L1_3
  end
  
  L6_2[1] = L7_2
  L6_2[2] = L8_2
  L8_2 = {}
  L8_2[1] = A0_2
  L2_2.ePlayerTankRepaired = L3_2(L4_2, L5_2, L6_2, OnPlayerTankRepair, L8_2)
  L2_2 = Object.IsPlayerControlled(A1_2)
  if L2_2 then
    A0_2.OnPlayerEnterVehicle(A0_2, A1_2)
  else
    A0_2.OnPlayerExitVehicle(A0_2, A1_2)
  end
end

SetupPlayerTank = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  MrxTutorialManager.ShowMessage("[DLCCon003.UI.TDestroyed]")
  A0_2.CleanupPlayerTankEvents(A0_2)
  nCurTankHealth = (nCurTankHealth + 1)
  A0_2.TankLife(A0_2, uPlayer)
  L2_2 = nCurTankHealth
  if L2_2 <= 3 then
    A0_2.SpawnPlayerTank(A0_2, A1_2)
  else
    A0_2.NoReplacement(A0_2)
  end
end

OnPlayerTankDeath = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  A0_2.CleanupPlayerTankEvents(A0_2)
  A0_2.SetupPlayerTank(A0_2, A1_2[1])
end

OnPlayerTankRepair = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  Event.Delete(A0_2._tEvents.ePlayerTankDeath)
  Event.Delete(A0_2._tEvents.ePlayerTankRepaired)
  Event.Delete(A0_2._tEvents.eEnterUberVeh)
end

CleanupPlayerTankEvents = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L3_2 = A0_2
  L2_2 = A0_2.CreateChild
  L4_2 = {}
  L4_2.sName = "Enter car"
  L4_2.sModuleName = "MrxTaskObjectiveEnterVehicle"
  L4_2.sDspShortDesc = "[MecCon001.Objectives.enterVehicle]"
  L4_2.vTgtInclude = A1_2
  L4_2.nQuota = 1
  L4_2.bDspBlp = true
  
  function L5_2()
    local L0_3, L1_3, L2_3
    L0_3 = A0_2
    L0_3.OnPlayerEnterVehicle(L0_3, A1_2)
  end
  
  L4_2.fOnComplete = L5_2
  L2_2 = L2_2(L3_2, L4_2)
end

OnPlayerExitVehicle = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L2_2 = A0_2._tEvents
  L6_2 = {}
  L6_2[1] = Player.GetAnyCharacter()
  L6_2[2] = A1_2
  L6_2[3] = "a"
  L6_2[4] = "x"
  L8_2 = {}
  L8_2[1] = A0_2
  L8_2[2] = A1_2
  L2_2.eEnterUberVeh = A0_2._CreateEvent(A0_2, Event.ObjectInSeat, L6_2, OnPlayerExitVehicle, L8_2)
end

OnPlayerEnterVehicle = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2
  L3_2 = Player.GetPrimaryCharacter()
  L5_2 = true
  L1_2 = A0_2.FindIntersection(A0_2, L3_2, 135, L5_2)
  L2_2 = Object.GetPosition
  L3_2 = Pg.GetGuidByName
  L4_2 = L1_2
  L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2 = L3_2(L4_2)
  L2_2, L3_2, L4_2 = L2_2(L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2)
  L7_2 = Pg.Spawn("TankBuster_Instant", (L2_2 + (math.randi(10) - 5)), L3_2, (L4_2 + (math.randi(10) - 5)), 0, false, true)
  L8_2 = {}
  L8_2[1] = "Fiona-In-Mission-Contract-Dlc03-04"
  L8_2[2] = "Fiona-In-Mission-Contract-Dlc03-05"
  L8_2[3] = "Fiona-In-Mission-Contract-Dlc03-06"
  L8_2[4] = "Fiona-In-Mission-Contract-Dlc03-07"
  L9_2 = {}
  L9_2[1] = "Fiona-In-Mission-Contract-Dlc03-13"
  L9_2[2] = "Fiona-In-Mission-Contract-Dlc03-14"
  L9_2[3] = "Fiona-In-Mission-Contract-Dlc03-15"
  L9_2[4] = "Fiona-In-Mission-Contract-Dlc03-16"
  tTBPickupVO = L9_2
  table.insert(tSpawned, L7_2)
  L9_2 = Minimap
  L9_2.AddObjectiveWithGuid(L9_2, ("TankBuster_" .. tostring(L7_2)), L7_2, 0, 0, 0, 51, 204, 153, 8, 8, "radar_Munition", true, false, false, 2)
  L9_2 = Hud.Radar
  L11_2 = {}
  L11_2.sName = ("TankBuster_" .. tostring(L7_2))
  L11_2.nDuration = 5
  L11_2.nMaxWidth = 12
  L11_2.nMaxHeight = 12
  L11_2.nSpeedWidth = 45
  L11_2.nSpeedHeight = 45
  L9_2.AnimateObjectiveSize(L9_2, L11_2)
  L9_2 = Hud.MessageBox
  L11_2 = {}
  L11_2.sMessage = "[DLCCon003.Terms.tankBuster]"
  L9_2.AddMessage(L9_2, L11_2)
  L10_2 = A0_2
  L9_2 = A0_2._CreateEvent
  L11_2 = Event.ObjectDelete
  L12_2 = {}
  L12_2[1] = L7_2
  
  function L13_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3
    L1_3 = Minimap
    L1_3.DeleteObjective(L1_3, ("TankBuster_" .. tostring(L7_2)))
  end
  
  L14_2 = {}
  L14_2[1] = A0_2
  eRemoveTBBlinky = L9_2(L10_2, L11_2, L12_2, L13_2, L14_2)
  nGiveTB = (nGiveTB + 1)
  A0_2.NotifyPlayerVO(A0_2, L8_2, 1)
  L12_2 = {}
  L12_2[1] = Player.GetAnyCharacter()
  L12_2[2] = L7_2
  L12_2[3] = "<"
  L12_2[4] = 8
  L12_2[5] = false
  L12_2[6] = false
  L14_2 = {}
  L14_2[1] = A0_2
  L14_2[2] = tTBPickupVO
  L14_2[3] = 1
  A0_2._CreateEvent(A0_2, Event.ObjectProximity, L12_2, NotifyPlayerVO, L14_2)
end

TankBusterSpawn = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L3_2 = MrxUtil.GetRandomTableElement(A1_2)
  L4_2 = bFirstPickup
  if L4_2 then
    L4_2 = tTBPickupVO
    if A1_2 == L4_2 then
      bFirstPickup = false
      L5_2 = A0_2
      L4_2 = A0_2._CreateEvent
      L6_2 = Event.TimerRelative
      L7_2 = {}
      L7_2[1] = A2_2
      
      function L8_2()
        local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3
        L1_3 = {}
        L3_3 = {}
        L5_3 = {}
        L5_3[1] = A0_2
        L3_3[1] = A0_2.ShowAirstrikeTutorial
        L3_3[2] = L5_3
        L1_3[1] = L3_2
        L1_3[2] = L3_3
        MrxVoSequence.Start(L1_3)
      end
      
      L4_2(L5_2, L6_2, L7_2, L8_2)
      return
    end
  end
  L5_2 = A0_2
  L4_2 = A0_2._CreateEvent
  L6_2 = Event.TimerRelative
  L7_2 = {}
  L7_2[1] = A2_2
  
  function L8_2()
    local L0_3, L1_3, L2_3
    L1_3 = {}
    L1_3[1] = L3_2
    MrxVoSequence.Start(L1_3)
  end
  
  L4_2(L5_2, L6_2, L7_2, L8_2)
end

NotifyPlayerVO = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  MrxTutorialManager.ShowMessage("[Tutorial.DlcCon003_Airstrike]")
  L4_2 = {}
  L4_2[1] = 10
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, MrxTutorialManager.HideMessage)
end

ShowAirstrikeTutorial = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L1_2 = {}
  L1_2[1] = "Fiona-In-Mission-Contract-Dlc03-32"
  L1_2[2] = "Fiona-In-Mission-Contract-Dlc03-33"
  L2_2 = ObjectFilter.Create()
  ObjectFilter.SetFilter(L2_2, "China && FlyingVehicle")
  L4_2 = A0_2
  L3_2 = A0_2._CreateEvent
  L5_2 = Event.ObjectDeath
  L6_2 = {}
  L6_2[1] = L2_2
  
  function L7_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3
    nBonusKills = (nBonusKills + 1)
    A0_3.nHeliBonusMoney = (A0_3.nHeliBonusMoney + knHeliBonus)
    MrxPmc.AddCashQty(knHeliBonus, nil, "[DLCCon003.Display.scoreBonus]")
    A0_3.NotifyPlayerVO(A0_3, L1_2, 0.5)
    A0_3.BonusTarget(A0_3)
  end
  
  L8_2 = {}
  L8_2[1] = A0_2
  uBonusEvent = L3_2(L4_2, L5_2, L6_2, L7_2, L8_2)
end

BonusTarget = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  if A1_2 == false then
    L2_2 = Player.GetSecondaryPlayer()
    L7_2 = true
    L3_2 = A0_2.FindIntersection(A0_2, uSpwnTnk, 80, L7_2)
    L4_2 = Object.GetPosition
    L5_2 = Pg.GetGuidByName
    L6_2 = L3_2
    L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2 = L5_2(L6_2)
    L4_2, L5_2, L6_2 = L4_2(L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2)
    A0_2.TankRespawn(A0_2, Pg.Spawn("DLC_M1A3", L4_2, L5_2, L6_2, nTankFace02, false, true))
    A0_2.TankLife(A0_2, L2_2)
  end
  L3_2 = A0_2
  L2_2 = A0_2._CreateEvent
  L4_2 = Event.ScriptEvent
  L5_2 = {}
  L6_2 = "mpPlayerLeft"
  
  function L7_2(A0_3)
    local L1_3, L2_3
    L1_3 = Net.IsServer()
    if L1_3 then
      L1_3 = not Player.IsLocal(A0_3[1])
    end
    return L1_3
  end
  
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L7_2 = {}
  L7_2[1] = A0_2
  eClientLeft = L2_2(L3_2, L4_2, L5_2, MultiplayerOff, L7_2)
end

MultiplayerOn = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L2_2 = A0_2
  L1_2 = A0_2._CreateEvent
  L3_2 = Event.ScriptEvent
  L4_2 = {}
  L5_2 = "mpPlayerJoin"
  
  function L6_2(A0_3)
    local L1_3, L2_3
    L1_3 = Net.IsServer()
    if L1_3 then
      L1_3 = not Player.IsLocal(A0_3[1])
    end
    return L1_3
  end
  
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L6_2 = {}
  L6_2[1] = A0_2
  L6_2[2] = false
  eClientJoined = L1_2(L2_2, L3_2, L4_2, MultiplayerOn, L6_2)
end

MultiplayerOff = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L3_2 = {}
  L3_2.sName = "DLCCon003_InitialObj"
  L3_2.sModuleName = "MrxTaskObjective"
  L3_2.sDspShortDesc = "[DLCCon003.Objectives.003]"
  L3_2.bDsp = true
  L4_2 = {}
  L5_2 = {}
  L7_2 = {}
  L7_2[1] = A0_2
  L5_2[1] = GlitteringPrizes
  L5_2[2] = L7_2
  L4_2[1] = L5_2
  L3_2.tOnComplete = L4_2
  L4_2 = {}
  L5_2 = {}
  L7_2 = {}
  L7_2[1] = A0_2
  L5_2[1] = A0_2.Cancel
  L5_2[2] = L7_2
  L4_2[1] = L5_2
  L3_2.tOnCancel = L4_2
  oInitialObj = A0_2.CreateChild(A0_2, L3_2)
  L2_2 = {}
  L6_2 = {}
  L8_2 = {}
  L8_2[1] = A0_2
  L6_2[1] = A0_2.ShowAirstrikeTutorial
  L6_2[2] = L8_2
  L9_2 = {}
  L11_2 = {}
  L11_2[1] = A0_2
  L9_2[1] = A0_2.ShowAmmoRepairTutorial
  L9_2[2] = L11_2
  L2_2[1] = "Fiona-In-Mission-Contract-Dlc03-01"
  L2_2[2] = 1
  L2_2[3] = "Fiona-In-Mission-Contract-Dlc03-02"
  L2_2[4] = L6_2
  L2_2[5] = 5
  L2_2[6] = "Fiona-In-Mission-Contract-Dlc03-03"
  L2_2[7] = L9_2
  MrxVoSequence.Start(L2_2)
  A0_2.StartTimer(A0_2, nTime)
  A0_2.BonusTarget(A0_2)
end

InitalizeObj = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  MrxTutorialManager.ShowMessage("[buildings.ammoBay.tutorial]")
  L4_2 = {}
  L4_2[1] = 10
  L6_2 = {}
  L6_2[1] = "[DLCCon003.UI.TLowHealth]"
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, MrxTutorialManager.ShowMessage, L6_2)
  L4_2 = {}
  L4_2[1] = 20
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, MrxTutorialManager.HideMessage, {})
end

ShowAmmoRepairTutorial = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = oMissionTimer
  if L2_2 then
  else
    L2_2 = MrxTimer
    L4_2 = {}
    L4_2.nStartTime = (A1_2 * 60)
    L4_2.iTray = 3
    L5_2 = {}
    L6_2 = {}
    L8_2 = {}
    L8_2[1] = oInitialObj
    L6_2[1] = oInitialObj.Complete
    L6_2[2] = L8_2
    L5_2[1] = L6_2
    L4_2.tDoneCallbacks = L5_2
    oMissionTimer = L2_2.Create(L2_2, L4_2)
    L2_2 = oMissionTimer
    L2_2.Start(L2_2)
  end
  L3_2 = A0_2
  L2_2 = A0_2._CreateEvent
  L4_2 = Event.TimerRelative
  L5_2 = {}
  L5_2[1] = ((A1_2 * 60) - 120)
  
  function L6_2(A0_3)
    local L1_3, L2_3, L3_3
    L2_3 = {}
    L2_3[1] = "Fiona-In-Mission-Contract-Dlc03-40"
    MrxVoSequence.Start(L2_3)
  end
  
  L7_2 = {}
  L7_2[1] = A0_2
  eTwoMinuteWarning = L2_2(L3_2, L4_2, L5_2, L6_2, L7_2)
end

StartTimer = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  L1_2 = oMissionTimer
  if L1_2 then
    L1_2 = oMissionTimer
    L1_2.Stop(L1_2)
    oMissionTimer = nil
  end
end

ClearTimer = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  A0_2.EarlyCleanup(A0_2)
  L1_2 = nGoal
  if 7 < L1_2 then
    sFinalVO = "Fiona-In-Mission-Contract-Dlc03-44"
  else
    sFinalVO = "Fiona-In-Mission-Contract-Dlc03-43"
  end
  L2_2 = {}
  L4_2 = {}
  L6_2 = {}
  L6_2[1] = A0_2
  L4_2[1] = A0_2.DisplayResults
  L4_2[2] = L6_2
  L2_2[1] = sFinalVO
  L2_2[2] = L4_2
  MrxVoSequence.Start(L2_2)
end

GlitteringPrizes = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2
  L2_2 = MrxPmc.GetCashQty() - (((A0_2.nTankMoney - A0_2.nEscapeMoney) - nRepairMoney) + A0_2.nHeliBonusMoney)
  L3_2 = ""
  if L2_2 < 0 then
    L3_2 = "-"
  end
  L4_2 = ""
  L5_2 = A0_2.nEscapeMoney
  if 0 < L5_2 then
    L4_2 = "-"
  end
  L5_2 = ""
  L6_2 = nRepairMoney
  if 0 < L6_2 then
    L5_2 = "-"
  end
  L6_2 = "[Scoring.DestroyedTanks] "
  L7_2 = MrxUtil.FormatMoney(A0_2.nTankMoney)
  L8_2 = "[n][Scoring.EscapedTanks] "
  L10_2 = MrxUtil.FormatMoney(A0_2.nEscapeMoney)
  L11_2 = "[n][Scoring.RepairCosts] "
  L13_2 = MrxUtil.FormatMoney(nRepairMoney)
  L14_2 = "[n][Scoring.Bonus] "
  L15_2 = MrxUtil.FormatMoney(A0_2.nHeliBonusMoney)
  L16_2 = "[n][Scoring.Misc]: "
  L19_2 = math.abs
  L20_2 = L2_2
  L19_2, L20_2, L21_2 = L19_2(L20_2)
  L18_2 = MrxUtil.FormatMoney(L19_2, L20_2, L21_2)
  L19_2 = "[n][green][Scoring.Total]: "
  L20_2 = MrxUtil.FormatMoney(MrxPmc.GetCashQty())
  L21_2 = "[n]"
  L6_2 = L6_2 .. L7_2 .. L8_2 .. L4_2 .. L10_2 .. L11_2 .. L5_2 .. L13_2 .. L14_2 .. L15_2 .. L16_2 .. L3_2 .. L18_2 .. L19_2 .. L20_2 .. L21_2
  L8_2 = Player.GetPrimaryPlayer()
  L10_2 = {}
  L11_2 = nil
  L12_2 = A0_2.Complete
  L13_2 = {}
  L13_2[1] = A0_2
  L14_2 = nil
  L15_2 = nil
  L16_2 = "center"
  L17_2 = "center"
  L18_2 = true
  L19_2 = nil
  A0_2.oScoreBoard = MrxGui.DisplayDialogBox(L8_2, L6_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2)
  L9_2 = MrxPmc.GetCashQty
  L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2 = L9_2()
  Net.LeaderboardPushScore("DlcCon003", L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2)
  L9_2 = MrxPmc.GetCashQty
  L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2 = L9_2()
  DLC01_MissionHub.SetPrevBest("DlcCon003", L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2)
end

DisplayResults = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  A0_2.EarlyCleanup(A0_2)
  A0_2._SetCancelMessage(A0_2, "[DLCCon003.Cancel.tooMany]")
  L2_2 = {}
  L4_2 = {}
  L6_2 = {}
  L6_2[1] = A0_2
  L4_2[1] = A0_2.Cancel
  L4_2[2] = L6_2
  L2_2[1] = "Fiona-In-Mission-Contract-Dlc03-41"
  L2_2[2] = L4_2
  MrxVoSequence.Start(L2_2)
end

TanksEscaped = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  A0_2.EarlyCleanup(A0_2)
  A0_2._SetCancelMessage(A0_2, "[DLCCon003.Cancel.noReplacements]")
  L2_2 = {}
  L4_2 = {}
  L6_2 = {}
  L6_2[1] = A0_2
  L4_2[1] = A0_2.Cancel
  L4_2[2] = L6_2
  L2_2[1] = "Fiona-In-Mission-Contract-Dlc03-42"
  L2_2[2] = L4_2
  MrxVoSequence.Start(L2_2)
end

NoReplacement = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  A0_2._SetCancelMessage(A0_2, "[DLCCon003.Cancel.bridgeDestroyed]")
  A0_2.Cancel(A0_2)
end

BridgeDeath = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  nTracerCountA = 0
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerFireOnA, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerFireOnA, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerFireOnA, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerFireOnA, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L6_2[1] = A0_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerFireOnA, L6_2)
end

TracerFireWakeUpA = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  nTracerCountB = 0
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerFireOnB, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerFireOnB, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerFireOnB, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L6_2[1] = A0_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerFireOnB, L6_2)
end

TracerFireWakeUpB = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  nExpCountA = 0
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerExplosionOnA, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerExplosionOnA, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerExplosionOnA, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L6_2[1] = A0_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerExplosionOnA, L6_2)
end

TracerFireExpA = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  nExpCountB = 0
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerExplosionOnB, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerExplosionOnB, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerExplosionOnB, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L6_2[1] = A0_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, TracerExplosionOnB, L6_2)
end

TracerFireExpB = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  nDExpCount = 0
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L7_2 = A0_2
  L6_2[1] = L7_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, DistExplosionOn, L6_2)
  L4_2 = {}
  L5_2 = MrxUtil.GetRandomTableElement
  L6_2 = tTracerTime
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  L4_2[1] = L5_2
  L4_2[2] = L6_2
  L4_2[3] = L7_2
  L6_2 = {}
  L6_2[1] = A0_2
  A0_2._CreateEvent(A0_2, Event.TimerRelative, L4_2, DistExplosionOn, L6_2)
end

DistExplosion = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  nTracerCountA = (nTracerCountA + 1)
  L1_2 = {}
  L1_2[1] = "loc_tracerfire_a"
  L1_2[2] = "loc_tracerfire_b"
  L1_2[3] = "loc_tracerfire_c"
  L1_2[4] = "loc_tracerfire_d"
  L1_2[5] = "loc_tracerfire_e"
  L1_2[6] = "loc_tracerfire_f"
  L1_2[7] = "loc_tracerfire_g"
  L1_2[8] = "loc_tracerfire_h"
  sRandomTracerA = MrxUtil.GetRandomTableElement(L1_2)
  TracerFireCommit(A0_2, sRandomTracerA)
end

TracerFireOnA = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  nTracerCountB = (nTracerCountB + 1)
  L1_2 = {}
  L1_2[1] = "loc_tracerfire_j"
  L1_2[2] = "loc_tracerfire_k"
  L1_2[3] = "loc_tracerfire_l"
  L1_2[4] = "loc_tracerfire_m"
  L1_2[5] = "loc_tracerfire_n"
  L1_2[6] = "loc_tracerfire_o"
  sRandomTracerB = MrxUtil.GetRandomTableElement(L1_2)
  TracerFireCommit(A0_2, sRandomTracerB)
end

TracerFireOnB = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  nExpCountA = (nExpCountA + 1)
  L1_2 = {}
  L1_2[1] = "loc_tracerexplosions_a"
  L1_2[2] = "loc_tracerexplosions_b"
  L1_2[3] = "loc_tracerexplosions_c"
  L1_2[4] = "loc_tracerexplosions_d"
  L1_2[5] = "loc_tracerexplosions_e"
  L1_2[6] = "loc_tracerexplosions_f"
  L1_2[7] = "loc_tracerexplosions_g"
  L1_2[8] = "loc_tracerexplosions_h"
  sRandomExplosionA = MrxUtil.GetRandomTableElement(L1_2)
  A0_2.TracerExplosionExec(A0_2, sRandomExplosionA)
end

TracerExplosionOnA = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  nExpCountB = (nExpCountB + 1)
  L1_2 = {}
  L1_2[1] = "loc_tracerexplosions_i"
  L1_2[2] = "loc_tracerexplosions_j"
  L1_2[3] = "loc_tracerexplosions_k"
  L1_2[4] = "loc_tracerexplosions_l"
  L1_2[5] = "loc_tracerexplosions_m"
  L1_2[6] = "loc_tracerexplosions_n"
  L1_2[7] = "loc_tracerexplosions_o"
  sRandomExplosionB = MrxUtil.GetRandomTableElement(L1_2)
  A0_2.TracerExplosionExec(A0_2, sRandomExplosionB)
end

TracerExplosionOnB = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  nDExpCount = (nDExpCount + 1)
  L1_2 = {}
  L1_2[1] = "loc_distexplosion_a"
  L1_2[2] = "loc_distexplosion_b"
  L1_2[3] = "loc_distexplosion_c"
  L1_2[4] = "loc_distexplosion_d"
  L1_2[5] = "loc_distexplosion_e"
  L1_2[6] = "loc_distexplosion_f"
  L1_2[7] = "loc_distexplosion_g"
  sRandomExp = MrxUtil.GetRandomTableElement(L1_2)
  A0_2.DistExplosionExec(A0_2, sRandomExp)
end

DistExplosionOn = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L2_2 = Pg.GetGuidByName("dlc_global_particle_tracer_AA")
  L3_2 = Pg.GetGuidByName(A1_2)
  L4_2 = Object.GetPosition
  L5_2 = L3_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  L7_2 = Object.GetYaw(L3_2)
  L9_2 = A0_2
  L8_2 = A0_2._CreateEvent
  L10_2 = Event.TimerRelative
  L11_2 = {}
  L11_2[1] = 1
  
  function L12_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3
    L1_3 = Pg.Spawn(L2_2, L4_2, L5_2, L6_2, L7_2, false, true)
  end
  
  eTracerFire = L8_2(L9_2, L10_2, L11_2, L12_2)
  L8_2 = nTracerCountA
  if L8_2 == 5 then
    L9_2 = A0_2
    L8_2 = A0_2._CreateEvent
    L10_2 = Event.TimerRelative
    L11_2 = {}
    L11_2[1] = 4
    
    function L12_2(A0_3)
      local L1_3, L2_3, L3_3
      Sound.CueSound(0, "Dlc_distCannons")
    end
    
    L8_2(L9_2, L10_2, L11_2, L12_2)
    L11_2 = {}
    L11_2[1] = 4
    L13_2 = {}
    L13_2[1] = A0_2
    A0_2._CreateEvent(A0_2, Event.TimerRelative, L11_2, TracerFireWakeUpA, L13_2)
  end
  L8_2 = nTracerCountB
  if L8_2 == 4 then
    L11_2 = {}
    L11_2[1] = 4
    L13_2 = {}
    L13_2[1] = A0_2
    A0_2._CreateEvent(A0_2, Event.TimerRelative, L11_2, TracerFireWakeUpB, L13_2)
  end
end

TracerFireCommit = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L2_2 = Pg.GetGuidByName("dlc_global_particle_airstrike_distance")
  L3_2 = Pg.GetGuidByName(A1_2)
  L4_2 = Object.GetPosition
  L5_2 = L3_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  L7_2 = Object.GetYaw(L3_2)
  L9_2 = A0_2
  L8_2 = A0_2._CreateEvent
  L10_2 = Event.TimerRelative
  L11_2 = {}
  L11_2[1] = 1
  
  function L12_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3
    L1_3 = Pg.Spawn(L2_2, L4_2, L5_2, L6_2, L7_2, false, true)
  end
  
  eTracerExp = L8_2(L9_2, L10_2, L11_2, L12_2)
  L8_2 = nExpCountA
  if L8_2 == 4 then
    L11_2 = {}
    L11_2[1] = 6
    L13_2 = {}
    L13_2[1] = A0_2
    A0_2._CreateEvent(A0_2, Event.TimerRelative, L11_2, TracerFireExpA, L13_2)
  end
  L8_2 = nExpCountB
  if L8_2 == 4 then
    L11_2 = {}
    L11_2[1] = 6
    L13_2 = {}
    L13_2[1] = A0_2
    A0_2._CreateEvent(A0_2, Event.TimerRelative, L11_2, TracerFireExpB, L13_2)
  end
end

TracerExplosionExec = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L2_2 = Pg.GetGuidByName("DLC_Explosion (Daisy Cutter)")
  L3_2 = Pg.GetGuidByName(A1_2)
  L4_2 = Object.GetPosition
  L5_2 = L3_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  L7_2 = Object.GetYaw(L3_2)
  L9_2 = A0_2
  L8_2 = A0_2._CreateEvent
  L10_2 = Event.TimerRelative
  L11_2 = {}
  L11_2[1] = 1
  
  function L12_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3
    L1_3 = Pg.Spawn(L2_2, L4_2, L5_2, L6_2, L7_2, false, true)
    L2_3 = nDExpCount
    if L2_3 == 2 then
      L5_3 = {}
      L5_3[1] = 30
      L7_3 = {}
      L7_3[1] = A0_3
      A0_3._CreateEvent(A0_3, Event.TimerRelative, L5_3, DistExplosion, L7_3)
    end
  end
  
  L13_2 = {}
  L13_2[1] = A0_2
  eTracerExp = L8_2(L9_2, L10_2, L11_2, L12_2, L13_2)
end

DistExplosionExec = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  A0_2.oScoreBoard = nil
  MrxTaskContract.Complete(A0_2)
end

Complete = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  A0_2.EarlyCleanup(A0_2)
  MrxTaskContract.Cancel(A0_2)
end

Cancel = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L1_2 = oMissionTimer
  if L1_2 then
    A0_2.ClearTimer(A0_2)
    oMissionTimer = nil
  end
  L1_2 = eTwoMinuteWarning
  if L1_2 then
    Event.Delete(eTwoMinuteWarning)
    eTwoMinuteWarning = nil
  end
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.vPlayer = nil
  L3_2.nSlot = 1
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.vPlayer = nil
  L3_2.nSlot = 2
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = ipairs
  L2_2 = tExitLocs
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = "Blip_" .. L5_2
    L7_2 = Hud.Radar
    L9_2 = {}
    L9_2.sName = L6_2
    L7_2.UnanimateObjective(L7_2, L9_2)
    L8_2 = Pg.GetGuidByName
    L9_2 = L5_2
    L8_2, L9_2, L10_2, L11_2, L12_2 = L8_2(L9_2)
    Marker.HaltPulse(L8_2, L9_2, L10_2, L11_2, L12_2)
    L7_2 = Minimap
    L7_2.DeleteObjective(L7_2, L6_2)
    L7_2 = Pda.Map
    L9_2 = {}
    L9_2.sName = ("PDA_Blip_" .. tostring(L5_2))
    L7_2.RemoveBlip(L7_2, L9_2)
  end
  L1_2 = tExitLocMarkers
  if L1_2 then
    L1_2 = ipairs
    L2_2 = tExitLocMarkers
    L1_2, L2_2, L3_2 = L1_2(L2_2)
    for L4_2, L5_2 in L1_2, L2_2, L3_2 do
      Marker.Remove(L5_2)
    end
    tExitLocMarkers = nil
  end
  L1_2 = a
  if L1_2 then
    L1_2 = a
    L1_2.Cleanup(L1_2)
    a = nil
  end
  L1_2 = pairs
  L2_2 = A0_2.tVehicleEvents
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    A0_2.CleanupEnemyTank(A0_2, L4_2)
  end
  MrxTutorialManager.HideMessage()
  L1_2 = A0_2._tEvents
  if L1_2 then
    L1_2 = pairs
    L2_2 = A0_2._tEvents
    L1_2, L2_2, L3_2 = L1_2(L2_2)
    for L4_2, L5_2 in L1_2, L2_2, L3_2 do
      Event.Delete(L5_2)
    end
  end
end

EarlyCleanup = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  WifVzBoundary.RemoveWorldBoundary()
  Player.SetVehicleDisguise(true)
  L1_2 = ipairs
  L2_2 = tSpawned
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = Object.HasLabel(L5_2, "Vehicle")
    if not L6_2 then
      L7_2 = Minimap
      L7_2.DeleteObjective(L7_2, ("New Tank" .. tostring(L5_2)))
      L7_2 = Minimap
      L7_2.DeleteObjective(L7_2, ("Alert_" .. tostring(L5_2)))
      L7_2 = Minimap
      L7_2.DeleteObjective(L7_2, ("TankBuster_" .. tostring(L5_2)))
    else
      L7_2 = Vehicle.GetDriver(L5_2)
      L8_2 = Object.IsPlayerControlled(L5_2)
      if not L8_2 then
        L10_2 = {}
        L10_2.AIGuid = L7_2
        L10_2.Handle = 0
        Ai.RemoveGoal(L10_2)
        L9_2 = Minimap
        L9_2.DeleteObjective(L9_2, ("New Tank" .. tostring(L5_2)))
        L9_2 = Minimap
        L9_2.DeleteObjective(L9_2, ("Alert_" .. tostring(L5_2)))
        L9_2 = Minimap
        L9_2.DeleteObjective(L9_2, ("TankBuster_" .. tostring(L5_2)))
      else
        Vehicle.Exit(L5_2, L7_2, true)
      end
    end
    Object.FadeOut(L5_2, 3, true)
  end
  L1_2 = A0_2.oScoreBoard
  if L1_2 then
    L1_2 = A0_2.oScoreBoard
    L1_2.Close(L1_2)
    A0_2.oScoreBoard = nil
  end
  MrxTaskContract.Cleanup(A0_2)
end

Cleanup = L0_1
