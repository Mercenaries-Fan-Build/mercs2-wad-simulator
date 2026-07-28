inherit("MrxTaskContract")
import("MrxSubtitle")
import("DangerousBuilding")
import("MrxLayerManager")
import("MrxUtil")
import("MrxTimer")
import("MrxPmc")
import("MrxSupportData")
import("MrxVoSequence")
import("MrxMusic")
import("mrxclusterbomb")
import("mrxfuelairbomb")
import("mrxcratedelivery")
import("mrxtankbuster")
DEMO = false
oCivCasualtyObjective = nil

function LoadAssets(self, tSaveData)
  local tLayersToAdd = {
    "vz_state_gurcon002",
    "vz_state_gurcon002_pristine",
    "Vz_state_temp_staging_GurCon002",
    "vz_state_merida_act1",
    "vz_state_gurcon002_traffic",
    "vz_State_GurCon002_TG"
  }
  MrxLayerManager.Remove({
    "vz_state_merida_act1_helo"
  })
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  self:_CreateEvent(Event.ObjectHibernation, {
    Player.GetLocalCharacter(),
    "awake"
  }, Start, {self})
end

function Start(self)
  SetupDangerousBuildings()
  tTankObjs = {}
  TankNum = 1
  DamageOnHibernate = 1
  ChurchLifeVO50 = 0
  ChurchLifeVO15 = 0
  BuildingsDestroyed = 0
  uChurchGuid = Pg.GetGuidByName("_merida_bld_plazachurch 0")
  uBonusEvent = nil
  if self:_GetFlag("ChurchDefended") then
    self:KillCaptain()
  elseif self:_GetFlag("AllBuildingsDestroyed") then
    self:MoveToChurch()
  else
    BuildingsDestroyed = 0
    _SetupVO(self)
    SetupDestroyObjective(self)
  end
end

function SetupDestroyObjective(self)
  SetupDangerousObjBuildings()
  self:CreateChild({
    sName = "DestroyBuildings",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "CommercialBuildingGurcon002",
      "ResidentialBuildingGurcon002",
      "ProjectsBuildingGurcon002"
    },
    sDspShortDesc = "[GurCon002.Objectives.001]",
    tOnComplete = {
      {
        self.CheckCompletion,
        {self}
      }
    },
    tOnPartComplete = {
      {
        BuildingDestroyedVO,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    },
    vVoSeqOnAdd = {
      "Fiona-Banter-Contract-Gur002-01",
      1,
      {
        mattias = "Mattias-Banter-Contract-Gur002-02",
        jennifer = "Jennifer-Banter-Contract-Gur002-03",
        chris = "Chris-Banter-Contract-Gur002-04"
      },
      1,
      "Fiona-In-Mission-Contract-Gur002-11",
      1,
      "Fiona-In-Mission-Contract-Gur002-12"
    }
  })
  if self:_GetFlag("BonusFailed") then
    self:_SetPlayer1Bonus(0)
    self:_SetPlayer2Bonus(0)
  else
    self:_SetPlayer1Bonus(500000)
    self:_SetPlayer2Bonus(500000)
    _SetupBonusObjective(self)
  end
  self:_CreateEvent(Event.TimerRelative, {45}, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-Contract-Gur002-53"
    }
  })
  nHealth = Object.GetHealth(uChurchGuid)
  uChurchHealthEvent = self:_CreateEvent(Event.ObjectHealth, {
    uChurchGuid,
    "<",
    nHealth
  }, function()
    oDontHurtChurch = self:CreateChild({
      sName = "DontHurtChurch",
      sModuleName = "MrxTaskObjective",
      sDspShortDesc = "[GurCon002.Objectives.008]",
      vVoSeqOnAdd = {
        "Fiona-In-Mission-Contract-Gur002-43"
      }
    })
  end)
  uChurchDeath = self:_CreateEvent(Event.ObjectDeath, {uChurchGuid}, function()
    self:_SetCancelMessage("[GurCon002.Terms.CancelChurchDead]")
    self.Cancel(self)
  end)
end

function SetupChurchObjective(self)
  self:_CreateEvent(Event.TimerRelative, {60}, MoveToChurch, {self})
end

function CheckCompletion(self)
  self:_SetFlag("AllBuildingsDestroyed")
  _Checkpoint()
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Gur002-35",
    {
      self.MoveToChurch,
      {self}
    }
  })
end

function MoveToChurch(self)
  oChurchFound = self:CreateChild({
    sName = "MoveToChurch",
    sDspShortDesc = "[GurCon002.Objectives.009]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "Church_GurCon002",
    vDestRegion = "LineRegion_Church",
    fDist = 5,
    bStop = false,
    bXZOnly = false,
    tOnComplete = {
      {
        self.DefendChurch,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    },
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Gur002-36"
    }
  })
end

function DefendChurch(self)
  if uChurchHealthEvent then
    Event.Delete(uChurchHealthEvent)
  end
  if uChurchDeath then
    Event.Delete(uChurchDeath)
  end
  _RemoveTanks(self)
  _ChurchHibernationCancel(self)
  uHibernationCancelEvent = self:_CreateEvent(Event.ObjectHibernation, {uChurchGuid, "asleep"}, function()
  end)
  Ai.SetInfractionMultiplier(GetGuidByName("Guerilla"), 0)
  oDefendChurchObj = self:CreateChild({
    sName = "DefendChurch",
    sModuleName = "MrxTaskObjectiveProtect",
    vTgtInclude = uChurchGuid,
    sDspShortDesc = "[GurCon002.Objectives.006]",
    tOnComplete = {
      {
        self.KillCaptain,
        {self}
      }
    },
    fOnCancel = function()
      if TracingVO1 then
        Event.Delete(TracingVO1)
      end
      if TracingVO2 then
        Event.Delete(TracingVO2)
      end
      if TracingVO3 then
        Event.Delete(TracingVO3)
      end
      if DirectionVO1 then
        Event.Delete(DirectionVO1)
      end
      if DirectionVO2 then
        Event.Delete(DirectionVO2)
      end
      if DirectionVO3 then
        Event.Delete(DirectionVO3)
      end
      if DirectionVO4 then
        Event.Delete(DirectionVO4)
      end
      if DirectionVO5 then
        Event.Delete(DirectionVO5)
      end
      if CountDownEvent then
        Event.Delete(CountDownEvent)
      end
      self:_SetCancelMessage("[GurCon002.Terms.CancelChurchUndefended]")
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Gur002-50",
        {
          self.Cancel,
          {self}
        }
      })
    end,
    vVoSeqOnAdd = {
      {
        mattias = "Mattias-In-Mission-Contract-Gur002-21",
        jennifer = "Jennifer-In-Mission-Contract-Gur002-22",
        chris = "Chris-In-Mission-Contract-Gur002-23"
      },
      1,
      "Fiona-In-Mission-Contract-Gur002-15",
      0
    },
    fOnInitialNotesComplete = function()
      MrxUtil.DisplayHealthBar(self, uChurchGuid, 0, true, 1)
      DisplayCountdownBar(self, 0)
    end
  })
  MrxMusic.PlaySpecialMusic("mu_fac_gr_kickass_01")
  Debug.Printf("music on")
  SetupChurchHealthVO(self)
  self:_CreateEvent(Event.TimerRelative, {20}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_CommSouth"
  })
  self:_CreateEvent(Event.TimerRelative, {20}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Gur002-52"
  })
  DirectionVO1 = self:_CreateEvent(Event.TimerRelative, {35}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Pmc03-66"
  })
  self:_CreateEvent(Event.TimerRelative, {40}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_CommNorth"
  })
  self:_CreateEvent(Event.TimerRelative, {50}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_South"
  })
  DirectionVO2 = self:_CreateEvent(Event.TimerRelative, {85}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Oil01-89"
  })
  self:_CreateEvent(Event.TimerRelative, {80}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_SoccerMiddle_Tank"
  })
  self:_CreateEvent(Event.TimerRelative, {90}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_SoccerNorth_Tank"
  })
  self:_CreateEvent(Event.TimerRelative, {110}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_SouthStairs"
  })
  ReinforcementVO1 = self:_CreateEvent(Event.TimerRelative, {137}, MrxVoSequence.Start, {
    "Fiona.vg2fio14"
  })
  self:_CreateEvent(Event.TimerRelative, {135}, _GurHeloDrop, {
    self,
    Pg.GetGuidByName("GurCon001HeloDropPoint1"),
    Pg.GetGuidByName("GurCon002HeloPath1"),
    Pg.GetGuidByName("GurCon001_ChurchHoverPath1")
  })
  DirectionVO3 = self:_CreateEvent(Event.TimerRelative, {160}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Oil01-97"
  })
  self:_CreateEvent(Event.TimerRelative, {150}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_SoccerSouth_Tank"
  })
  self:_CreateEvent(Event.TimerRelative, {150}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_CommNorth_APC"
  })
  self:_CreateEvent(Event.TimerRelative, {170}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_Outpost"
  })
  ReinforcementVO2 = self:_CreateEvent(Event.TimerRelative, {193}, MrxVoSequence.Start, {
    "Fiona-In-Mission-MinorContract-Gur04-04"
  })
  self:_CreateEvent(Event.TimerRelative, {190}, _GurHeloDrop, {
    self,
    Pg.GetGuidByName("GurCon001HeloDropPoint2"),
    Pg.GetGuidByName("GurCon002HeloPath1"),
    Pg.GetGuidByName("GurCon001_ChurchHoverPath2")
  })
  DirectionVO4 = self:_CreateEvent(Event.TimerRelative, {220}, MrxVoSequence.Start, {
    "Fiona.Alex07"
  })
  self:_CreateEvent(Event.TimerRelative, {210}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_CommSouthStairs"
  })
  self:_CreateEvent(Event.TimerRelative, {210}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_SouthStairs"
  })
  TracingVO1 = self:_CreateEvent(Event.TimerRelative, {268}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Gur002-17"
  })
  DirectionVO5 = self:_CreateEvent(Event.TimerRelative, {255}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Pmc03-67"
  })
  self:_CreateEvent(Event.TimerRelative, {250}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_SoccerMiddle_Tank"
  })
  self:_CreateEvent(Event.TimerRelative, {250}, _SpawnTankOutOfView, {
    self,
    "Path_ChurchAttack_SoccerSouth_Tank"
  })
  TracingVO2 = self:_CreateEvent(Event.TimerRelative, {288}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Gur002-18"
  })
  TracingVO3 = self:_CreateEvent(Event.TimerRelative, {300}, MrxVoSequence.Start, {
    "Fiona-In-Mission-Contract-Gur002-19"
  })
end

function _ChurchDead(self)
  Debug.Printf([[
-------------------------------------------------
The Church is Dead
---------------------------------------]])
  MrxUtil.StopHealthBar(uChurchGuid)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Gur002-24"
  })
end

function KillCaptain(self)
  self:_SetFlag("ChurchDefended")
  _Checkpoint()
  MrxMusic.StopSpecialMusic("none")
  Debug.Printf("music off")
  if uAbandonCancelEvent then
    Event.Delete(uAbandonCancelEvent)
  end
  Ai.SetInfractionMultiplier(GetGuidByName("Guerilla"), 1)
  if oDontHurtChurch then
    oDontHurtChurch:Complete()
  end
  if uChurchVOHealthEvent1 then
    Event.Delete(uChurchVOHealthEvent1)
  end
  if uChurchVOHealthEvent2 then
    Event.Delete(uChurchVOHealthEvent2)
  end
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("GurCon002_Soccer_Music_Border"),
    "enter"
  }, _SetupSoccerMusic, {self})
  Event.Delete(uChurchDeath)
  MrxUtil.StopHealthBar(uChurchGuid)
  Mendez_Spawn = Pg.Spawn("VZ Deathsquad B HVT", 2674.283, -29.048033, -1654.2429, 0, false, true)
  local bSuccess = Object.SetName(Mendez_Spawn, "Mendez")
  self:CreateChild({
    sName = "VerifyMendez",
    sModuleName = "MrxTaskObjectiveVerify",
    vTgtInclude = Mendez_Spawn,
    sDspShortDesc = "[GurCon002.Objectives.007]",
    sFactionId = "Gur",
    fOnComplete = function()
      MrxVoSequence.Start({
        {
          "Fiona-In-Mission-Contract-Gur002-20"
        },
        {
          self.Complete,
          {self}
        }
      })
    end,
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    },
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Gur002-39",
      "Fiona-In-Mission-Contract-Gur002-40",
      "Fiona-In-Mission-Contract-Gur002-41"
    }
  })
end

function _SetupSoccerMusic(self)
  MrxMusic.PlaySpecialMusic("mu_fac_gr_threat_01")
  Debug.Printf("music on")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("GurCon002_Soccer_Music_Border"),
    "exit"
  }, function()
    MrxMusic.StopSpecialMusic("none")
    Debug.Printf("music off")
    self:_CreateEvent(Event.Boundary, {
      Player.GetAnyCharacter(),
      Pg.GetGuidByName("GurCon002_Soccer_Music_Border"),
      "Enter"
    }, function()
      MrxMusic.PlaySpecialMusic("mu_fac_gr_threat_01")
      Debug.Printf("music on")
      _SetupSoccerMusic(self)
    end)
  end)
end

function _SetupBonusObjective(self)
  Debug.Printf("inside _SetupBonusObjective")
  oCivCasualtyObjective = self:CreateChild({
    sName = "Civ Death Bonus",
    sModuleName = "MrxTaskObjective",
    bOptional = true,
    bDspMsg = true,
    bDspDescPda = true,
    bDspBlp = false,
    sDspShortDesc = "[GurCon002.Objectives.Bonus]"
  })
  uBonusEvent = self:_CreateEvent(Event.ScriptEvent, {
    "CollateralDamage",
    function()
      return true
    end
  }, function()
    self:_SetPlayer1Bonus(0)
    self:_SetPlayer2Bonus(0)
    Debug.Printf("Cancelling Objective, bonus has been set to 0")
    oCivCasualtyObjective:Cancel()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Gur002-54"
    })
    self:_SetFlag("BonusFailed")
    _Checkpoint()
  end)
end

function _RemoveTanks(self)
  Debug.Printf("Inside _RemoveTanks")
  local tTanksNearChurch = Pg.FastCollectGroundVehicles(2113, -7, -1547, 250, "amx30")
  for i, uTank in ipairs(tTanksNearChurch) do
    Debug.Printf("Looking at tank " .. tostring(uTank))
    local uDriver = Vehicle.GetDriver(uTank)
    local sFaction = MrxUtil.GetFaction(uDriver)
    if sFaction == "VZ" then
      Debug.Printf("We found a tank not driven by the VZ!")
      if not Object.IsVisible(uTank) then
        Object.Remove(uTank)
      end
    end
  end
end

function Cleanup(self)
  Ai.RemoveExclusionZone()
  DangerousBuilding.SetRarity("all", "default")
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  Ai.SetInfractionMultiplier(GetGuidByName("Guerilla"), 1)
  if uAbandonCancelEvent then
    Event.Delete(uAbandonCancelEvent)
  end
  MrxMusic.StopSpecialMusic("none")
  if uBonusEvent then
    Event.Delete(uBonusEvent)
  end
  DangerousBuilding.RemoveDB(tMySpawners)
  MrxLayerManager.Remove("vz_state_gurcon002_traffic")
  MrxUtil.StopHealthBar(uGuid)
  if oTimer then
    oTimer:Stop()
  end
  MrxTaskContract.Cleanup(self)
end

function SetupDangerousObjBuildings()
  DangerousBuilding.SetRarity("default", "never")
  local SpawnDelay = 20
  TallCommBuild = Pg.GetGuidByName("CommercialBuildingGurcon002")
  Church = Pg.GetGuidByName("_merida_bld_plazachurch 0")
  ResidentialBuild = Pg.GetGuidByName("ResidentialBuildingGurcon002")
  NEBuild = Pg.GetGuidByName("ProjectsBuildingGurcon002")
  DangerousBuilding.TurnOn(TallCommBuild, false, true)
  Ai.TweakAttachedSpawnersInGroup(TallCommBuild, "Ground", {
    SpawnList = "Spawnlist (VZ Ground)"
  })
  Ai.TweakAttachedSpawnersInGroup(TallCommBuild, "Balcony", {
    SpawnList = "Spawnlist (VZ Ground)"
  })
  Ai.TweakAttachedSpawnersInGroup(TallCommBuild, "Rooftop", {
    SpawnList = "Spawnlist (VZ AA)"
  })
  Ai.TweakAttachedSpawners(TallCommBuild, {SpawnerState = "on"})
  Ai.TweakAttachedSpawners(TallCommBuild, {SecondsPerCycle = SpawnDelay})
  DangerousBuilding.TurnOn(ResidentialBuild, false, true)
  Ai.TweakAttachedSpawners(ResidentialBuild, {SpawnerState = "on"})
  Ai.TweakAttachedSpawnersInGroup(ResidentialBuild, "Ground", {
    SpawnList = "Spawnlist (VZ Ground)"
  })
  Ai.TweakAttachedSpawnersInGroup(ResidentialBuild, "Balcony", {
    SpawnList = "Spawnlist (VZ AA)"
  })
  Ai.TweakAttachedSpawnersInGroup(ResidentialBuild, "Rooftop", {
    SpawnList = "Spawnlist (VZ AA)"
  })
  Ai.TweakAttachedSpawners(ResidentialBuild, {SecondsPerCycle = SpawnDelay})
  DangerousBuilding.TurnOn(NEBuild, false, true)
  Ai.TweakAttachedSpawners(NEBuild, {SpawnerState = "on"})
  Ai.TweakAttachedSpawnersInGroup(NEBuild, "Balcony", {
    SpawnList = "Spawnlist (VZ AA)"
  })
  Ai.TweakAttachedSpawnersInGroup(NEBuild, "Rooftop", {
    SpawnList = "Spawnlist (VZ AA)"
  })
  Ai.TweakAttachedSpawnersInGroup(NEBuild, "Ground", {
    SpawnList = "Spawnlist (VZ Ground)"
  })
  DangerousBuilding.SetProperties(ResidentialBuild, {Density = 40})
  Ai.TweakAttachedSpawners(NEBuild, {SecondsPerCycle = SpawnDelay})
  tMySpawners = {ResidentialBuild}
  DangerousBuilding.SetProperties(ResidentialBuild, {Group = "Rooftop", Density = 0})
  DangerousBuilding.SetProperties(ResidentialBuild, {Group = "Balcony", Density = 0})
end

function SetupDangerousBuildings()
  local SpawnDelay = 60
  LockerWest = Pg.GetGuidByName("_merida_bld_lockerroom 0")
  LockerEast = Pg.GetGuidByName("_merida_bld_lockerroom 0x000b0952")
  MediaNE = Pg.GetGuidByName("_merida_bld_mediabooth 0x000b0a4d")
  MediaSW = Pg.GetGuidByName("_merida_bld_mediabooth 0x000b0a4f")
  BarracksEast = Pg.GetGuidByName("_vzoutpost_bld_barracktent 0x000b1ce8")
  BarracksWest = Pg.GetGuidByName("_vzoutpost_bld_barracktent 0x000b1ce9")
  DangerousBuilding.TurnOn(MediaNE, false, true)
  Ai.TweakAttachedSpawners(MediaNE, {SpawnerState = "on"})
  Ai.TweakAttachedSpawners(MediaNE, {
    SpawnList = "Spawnlist (VZ AA)"
  })
  Ai.TweakAttachedSpawners(MediaNE, {SecondsPerCycle = SpawnDelay})
  DangerousBuilding.TurnOn(MediaSW, false, true)
  Ai.TweakAttachedSpawners(MediaSW, {SpawnerState = "on"})
  Ai.TweakAttachedSpawners(MediaSW, {
    SpawnList = "Spawnlist (VZ AA)"
  })
  Ai.TweakAttachedSpawners(MediaSW, {SecondsPerCycle = SpawnDelay})
  DangerousBuilding.TurnOn(LockerEast, true, true)
  Ai.TweakAttachedSpawners(LockerEast, {
    SpawnList = "Spawnlist (VZ Elite)"
  })
  Ai.TweakAttachedSpawners(LockerEast, {SpawnerState = "on"})
  Ai.TweakAttachedSpawners(LockerEast, {SecondsPerCycle = SpawnDelay})
  DangerousBuilding.TurnOn(BarracksEast, true, true)
  Ai.TweakAttachedSpawners(BarracksEast, {
    SpawnList = "Spawnlist (VZ Elite)"
  })
  Ai.TweakAttachedSpawners(BarracksEast, {SpawnerState = "on"})
  Ai.TweakAttachedSpawners(BarracksEast, {SecondsPerCycle = SpawnDelay})
  DangerousBuilding.TurnOn(BarracksWest, true, true)
  Ai.TweakAttachedSpawners(BarracksWest, {
    SpawnList = "Spawnlist (VZ Elite)"
  })
  Ai.TweakAttachedSpawners(BarracksWest, {SpawnerState = "on"})
  Ai.TweakAttachedSpawners(BarracksWest, {SecondsPerCycle = SpawnDelay})
  ProtestSE = Pg.GetGuidByName("_merida_bld_universitydorm 0")
  ProtestNE = Pg.GetGuidByName("_merida_bld_universitylibrary 1")
  ProtestSW = Pg.GetGuidByName("_merida_bld_universitycampus 0x000e3dbf")
  ProtestNW = Pg.GetGuidByName("_merida_bld_universitydorm 0x000e3833")
  DangerousBuilding.TurnOn(uChurchGuid, true, true)
  Ai.TweakAttachedSpawners(uChurchGuid, {
    SpawnList = "Spawnlist (Guerilla Ground)"
  })
  Ai.TweakAttachedSpawners(uChurchGuid, {SpawnerState = "on"})
  Ai.TweakAttachedSpawners(uChurchGuid, {SecondsPerCycle = 20})
end

function _GurHeloDrop(self, uPointName, uPathName, uPatrolPath)
  Debug.Printf("Inside _GurHeloDrop")
  bSuccess, x, y, z, nFacing = MrxUtil.FindSpawnPointOutOfView(uPathName, 200)
  if bSuccess == true then
    SPAWN = Pg.Spawn("UH1 Transport (GR) (Full) (RPG)", x, y, z, nFacing, false, true)
    Debug.Printf("Inside _GurHeloDrop, helo spawned")
    local uDriver = Vehicle.GetDriver(SPAWN)
    Debug.Printf("Spawned vehicle driver = " .. tostring(SPAWN))
    tLandGoalParams = {
      AIGuid = uDriver,
      Goal = "HeliLand",
      Target = uPointName,
      Priority = "HiPri",
      Haste = 0.75,
      Force = true,
      Callback = BailOut,
      CallbackData = {
        self,
        SPAWN,
        uDriver,
        uPatrolPath
      }
    }
    self:_CreateEvent(Event.ObjectHibernation, {SPAWN, "awake"}, Ai.Goal, {tLandGoalParams})
  end
end

function BailOut(self, uHeli, uDriver, uPatrolPath)
  local tRiders = Vehicle.GetRiders(uHeli, "p")
  Ai.Deploy({
    Vehicle = uHeli,
    Role = "Passenger",
    Force = true,
    MaintainRotorSpeed = true,
    Callback = PatrolChurch,
    CallbackData = {
      self,
      uHeli,
      uDriver,
      uPatrolPath
    }
  })
end

function PatrolChurch(self, uHeli, uDriver, uPatrolPath)
  Ai.Goal({
    AIGuid = uDriver,
    Goal = "PathMove",
    Target = uPatrolPath,
    Start = "Nearest",
    Mode = "Loop",
    Haste = 0.2
  })
end

function _SpawnTankOutOfView(self, sPath)
  bSuccess, x, y, z, nFacing = MrxUtil.FindSpawnPointOutOfView(Pg.GetGuidByName(sPath), 200)
  Debug.Printf("@@@@ Success of spawning tank for path " .. sPath .. " = " .. tostring(bSuccess))
  if bSuccess == true then
    CoinToss = Math.randi(0, 1)
    SPAWN = Pg.Spawn("Amx30 (Full)", x, y, z, nFacing, false, true)
    _AttackChurch(self, SPAWN, Pg.GetGuidByName(sPath))
  end
end

function _AttackChurch(self, uActor, uTarget)
  local uDriver = Vehicle.GetDriver(uActor)
  Debug.Printf("Spawned vehicle driver = " .. tostring(uActor))
  tMoveGoalParams = {
    AIGuid = uDriver,
    Goal = "PathMove",
    Target = uTarget,
    Start = "Nearest",
    Priority = "hipri",
    Mode = "Oneway",
    Haste = 0.75,
    Callback = _FireOnChurch,
    CallbackData = {self, uDriver}
  }
  self:_CreateEvent(Event.ObjectHibernation, {uActor, "awake"}, Ai.Goal, {tMoveGoalParams})
  Debug.Printf(tostring(uActor) .. " is moving along path " .. tostring(uTarget))
end

function _FireOnChurch(self, uActor)
  CoinToss = Math.randi(0, 1)
  if CoinToss == 0 then
    local sPriority = "HiPri"
  else
    local sPriority = "MedPri"
  end
  tGoalParams = {
    AIGuid = uActor,
    Goal = "Attack",
    Target = uChurchGuid,
    Priority = sPriority
  }
  Ai.Goal(tGoalParams)
end

function _SpawnAPCOutOfView(self, sPath)
  bSuccess, x, y, z, nFacing = MrxUtil.FindSpawnPointOutOfView(Pg.GetGuidByName(sPath), 200)
  Debug.Printf("@@@@ Success of spawning tank for path " .. sPath .. " = " .. tostring(bSuccess))
  if bSuccess == true then
    SPAWN = Pg.Spawn("M113 (VZ) (Full)", x, y, z, nFacing, false, true)
    _APCMoveToChurch(self, SPAWN, Pg.GetGuidByName(sPath))
  end
end

function _APCMoveToChurch(self, uAPC, uPath)
  local uDriver = Vehicle.GetDriver(uAPC)
  Debug.Printf("Spawned vehicle driver = " .. tostring(uActor))
  tMoveGoalParams = {
    AIGuid = uDriver,
    Goal = "PathMove",
    Target = uPath,
    Start = "Nearest",
    Priority = "hipri",
    Mode = "Oneway",
    Haste = 0.75,
    Callback = BailOutAPC,
    CallbackData = {
      self,
      uAPC,
      uDriver
    }
  }
  self:_CreateEvent(Event.ObjectHibernation, {uAPC, "awake"}, Ai.Goal, {tMoveGoalParams})
end

function BailOutAPC(self, uAPC, uDriver)
  local tRiders = Vehicle.GetRiders(uAPC, "p")
  Ai.Deploy({
    Vehicle = uAPC,
    Role = "Passenger",
    Force = true
  })
end

function DisplayCountdownBar(self, prog)
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 1,
    sText = "[GurCon002.Objectives.010][yellow][bar" .. prog .. "]"
  })
  prog = prog + 5
  Debug.Printf("prog = " .. prog)
  if prog == 100 then
    oDefendChurchObj:Complete()
  else
    CountDownEvent = self:_CreateEvent(Event.TimerRelative, {15}, DisplayCountdownBar, {self, prog})
  end
end

function _ChurchHibernationCancel(self)
  uAbandonCancelEvent = self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("LR_Church_Abandon"),
    "Exit"
  }, function()
    self:_SetCancelMessage("[GurCon002.Objectives.006]")
    self.Cancel(self)
  end)
end

function _DamageOnHibernate(self)
  oChurchHibernateEvent = self:_CreateEvent(Event.ObjectHibernation, {uChurchGuid, "hibernated"}, _ChurchDamageTimer, {self})
end

function _ChurchDamageTimer(self)
  oChurchTimer = self:_CreateEvent(Event.TimerRelative, {5}, _HurtChurch, {self})
end

function _HurtChurch(self)
  nChurchLife = nChurchLife - 250
  if nChurchLife <= 0 then
    Object.Kill(uChurchGuid)
    Event.Delete(oChurchTimer)
  end
  Event.Delete(oChurchAwakeEvent)
  oChurchAwakeEvent2 = self:_CreateEvent(Event.ObjectHibernation, {uChurchGuid, "awake"}, _SetChurchHealth, {self, nChurchLife})
  _ChurchDamageTimer(self, nChurchLife)
end

function _SetChurchHealth(self)
  nChurchDamage = Object.GetHealth(uChurchGuid) - nChurchLife
  Object.SetHealth(uChurchGuid, nChurchLife)
  oChurchHibernateEvent2 = self:_CreateEvent(Event.ObjectHibernation, {uChurchGuid, "hibernated"}, _ChurchDamageTimer, {
    self,
    Object.GetHealth(uChurchGuid)
  })
end

function SetupChurchHealthVO(self)
  nHealth = Object.GetHealth(uChurchGuid)
  uChurchVOHealthEvent1 = self:_CreateEvent(Event.ObjectHealth, {
    uChurchGuid,
    "<",
    nHealth / 2
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Gur002-48"
    })
  end)
  uChurchVOHealthEvent2 = self:_CreateEvent(Event.ObjectHealth, {
    uChurchGuid,
    "<",
    nHealth / 10
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Gur002-49"
    })
  end)
end

function _SetupVO(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("rgn_sfx_GurCon002"),
    "enter"
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-Contract-Gur002-31"
    }
  })
end

function BuildingDestroyedVO(self)
  if BuildingsDestroyed == 0 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Gur002-33"
    })
  elseif BuildingsDestroyed == 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Gur002-34"
    })
  end
  BuildingsDestroyed = BuildingsDestroyed + 1
end
