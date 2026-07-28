inherit("MrxTaskContract")
import("MrxSubtitle")
import("Munitions")
import("MrxVoSequence")
import("MrxPmc")
import("MrxAi")
import("MrxSupportData")
oExtractBB = nil
oDestroyBunker = nil
nBBExtracted = 0
nCallbackID = 0
NETEVENT_SETBBQTY = 0

function NetEventCallback(nType, tArgs)
  if nType == NETEVENT_SETBBQTY then
    MrxPmc.SetSupportQty("bunkerbuster", tArgs[1])
  end
end

function LoadAssets(self, tSaveData)
  local tLayersToAdd = {
    "Vz_State_JetCon001",
    "Vz_State_JetCon001_Pristine",
    "Vz_State_JetCon001_CP01"
  }
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  MrxSupportData.SetJetPilotRecruited(true)
  Vehicle.Usable(Pg.GetGuidByName("JetCon001_ScrambleCopter"), false)
  self:BeachRegionActivate()
  self:AASiteRegionActivate()
  self:BBWarningRegionActivate()
  self:BunkerIslandRegionActivate()
  if self:_GetFlag("JC001CP02") then
    local tLayersToRemove = {
      "Vz_State_JetCon001_CP01"
    }
    MrxLayerManager.Remove(tLayersToRemove)
    nBBExtracted = 3
    MrxPmc.SetSupportQty("bunkerbuster", 3)
    Net.SendCustomEvent("JetCon001", NETEVENT_SETBBQTY, {3})
    self:DestroyBunker()
  elseif self:_GetFlag("JC001CP01") then
    MrxPmc.SetSupportQty("bunkerbuster", 0)
    Net.SendCustomEvent("JetCon001", NETEVENT_SETBBQTY, {0})
    nBBExtracted = 0
    self:ExtractBB()
  else
    MrxPmc.SetSupportQty("bunkerbuster", 0)
    Net.SendCustomEvent("JetCon001", NETEVENT_SETBBQTY, {0})
    nBBExtracted = 0
    self:CheckpointRegionActivate()
    self:TravelMusicOnRegionActivate()
    self:ExtractBB()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Jet01-01",
      0.5,
      {
        mattias = "Mattias-Briefing-Cinematic-Carmona-14",
        jennifer = "Jennifer-Briefing-Cinematic-Carmona-15",
        chris = "Chris-Briefing-Cinematic-Carmona-16"
      },
      0.5,
      "Fiona-In-Mission-Contract-Pir051-02"
    })
  end
end

function CheckpointRegionActivate(self)
  Debug.Printf("********************* JETCON001 : CHECKPOINT REGION ACTIVE ")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_JetCon001_Checkpoint"),
    "enter",
    false
  }, CheckpointActivate, {self})
end

function CheckpointActivate(self)
  if self:_GetFlag("JC001CP01") or self:_GetFlag("JC001CP02") then
  else
    self:_SetFlag("JC001CP01")
    _Checkpoint({"CP01_P1", "CP01_P2"})
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Jet01-02"
    })
  end
end

function BeachRegionActivate(self)
  Debug.Printf("********************* JETCON001 : BEACH REGION ACTIVE ")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_JetCon001_SupplyBeachAssault"),
    "enter",
    false
  }, BeachAssault, {self})
end

function BeachAssault(self)
  Debug.Printf("********************* Jet CON 001: BEACH ASSAULT !!!")
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("JetCon001_BeachAssault_Tank01")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_JetCon001_SupplyBeachAmbush_Tank01"),
    Mode = "Loop",
    Priority = "medPri",
    Haste = 1
  })
end

function AASiteRegionActivate(self)
  Debug.Printf("********************* AA SITE REGION ACTIVE ")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_JetCon001_Supply_AASite01"),
    "enter",
    false
  }, AASiteRegionAssault, {self})
end

function AASiteRegionAssault(self)
  Debug.Printf("********************* JET CON 001: HERE COMES SOME PAIN TO DA AA SITE !!!")
  MrxVoSequence.Start({
    {
      mattias = "Mattias-In-Mission-Contract-Jet01-31",
      jennifer = "Jennifer-In-Mission-Contract-Jet01-32",
      chris = "Chris-In-Mission-Contract-Jet01-33"
    }
  })
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("JetCon001_Supply_AAJeep01")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("JetCon001_AASite01_Jeep_Path"),
    Priority = "hiPri",
    Haste = 0.5
  })
end

function BBWarningRegionActivate(self)
  Debug.Printf("********************* BB WARNING REGION ACTIVE ")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_JetCon001_BBWarning"),
    "enter",
    false
  }, BBWarningVO, {self})
end

function BBWarningVO(self)
  Debug.Printf("********************* BB WARNING REGION VO SHOULD BE PLAYING ")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Jet01-15"
  })
end

function CopterAttackRegionActivate(self)
  Debug.Printf("********************* BUNKER ISLAND REGION ACTIVE ")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_JetCon001_CopterAttack"),
    "enter",
    false
  }, self.CopterSpawn, {self})
end

function CopterSpawn(self)
  Debug.Printf("********************* COPTER ATTACK - PLAY RIDE OF THE VALKYRIES HERE! ")
  MrxLayerManager.Add("Vz_State_JetCon001_CopterAttack", self.CopterMove, {self})
end

function CopterMove(self)
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("JetCon001_AttackCopter")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_JetCon001_CopterAttack"),
    Start = "Nearest",
    Mode = "Oneway",
    Priority = "HiPri",
    Haste = 1,
    Callback = CopterAttack,
    CallbackData = {self}
  })
end

function CopterAttack(self)
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("JetCon001_AttackCopter")),
    Goal = "Attack",
    Target = Player.GetLocalCharacter()
  })
end

function BunkerIslandRegionActivate(self)
  Debug.Printf("********************* BUNKER ISLAND REGION ACTIVE ")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_JetCon001_BunkerIslandArrive"),
    "enter",
    false
  }, BunkerIslandArrive, {self})
end

function BunkerIslandArrive(self)
  Vehicle.Usable(Pg.GetGuidByName("JetCon001_ScrambleCopter"), true)
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("JetCon001_Jeep_BunkerIsland")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_JetCon001_Bunker_JeepPatrol01"),
    Priority = "medPri",
    Haste = 0.5
  })
end

function NearBunkerRegionActivate(self)
  Debug.Printf("********************* NEAR BUNKER REGION ACTIVE ")
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_JetCon001_NearBunker"),
    "enter",
    false
  }, NearBunkerVO, {self})
end

function NearBunkerVO(self)
  Debug.Printf("********************* MESSAGES ABOUT USING THE LASER DESIGNATOR ")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Jet01-43",
    0.5,
    "Fiona-In-Mission-Contract-Jet01-04"
  })
end

function ExtractBB(self)
  oExtractBB = self:CreateChild({
    sName = "JetCon001_BunkerBuster",
    sModuleName = "MrxTaskObjective",
    vTgtInclude = {
      Pg.GetGuidByName("JetCon001_BunkerBuster01"),
      Pg.GetGuidByName("JetCon001_BunkerBuster02"),
      Pg.GetGuidByName("JetCon001_BunkerBuster03")
    },
    nQuota = 3,
    vTgtExclude = {},
    sDspShortDesc = "[JetCon001.Objectives.001]",
    tOnComplete = {
      {
        DestroyBunker,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
  uBBDestroyed01 = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("JetCon001_BunkerBuster01")
  }, CancelExtractBB, {self})
  uBBDestroyed02 = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("JetCon001_BunkerBuster02")
  }, CancelExtractBB, {self})
  uBBDestroyed03 = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("JetCon001_BunkerBuster03")
  }, CancelExtractBB, {self})
  uBBExtract01 = self:_CreateEvent(Event.ScriptEvent, {
    "MunitionsPickup",
    function(tData)
      return Pg.GetGuidByName("JetCon001_BunkerBuster01") == tData[2]
    end
  }, BBPickup, {self, 1})
  uBBExtract02 = self:_CreateEvent(Event.ScriptEvent, {
    "MunitionsPickup",
    function(tData)
      return Pg.GetGuidByName("JetCon001_BunkerBuster02") == tData[2]
    end
  }, BBPickup, {self, 2})
  uBBExtract03 = self:_CreateEvent(Event.ScriptEvent, {
    "MunitionsPickup",
    function(tData)
      return Pg.GetGuidByName("JetCon001_BunkerBuster03") == tData[2]
    end
  }, BBPickup, {self, 3})
  Munitions.HideMarker(Pg.GetGuidByName("JetCon001_BunkerBuster01"))
  Munitions.HideMarker(Pg.GetGuidByName("JetCon001_BunkerBuster02"))
  Munitions.HideMarker(Pg.GetGuidByName("JetCon001_BunkerBuster03"))
end

function BBPickup(self, nBBindex)
  nBBExtracted = nBBExtracted + 1
  Net.SendCustomEvent("JetCon001", NETEVENT_SETBBQTY, {nBBExtracted})
  if uBBExtract01 then
    Event.Delete("uBBDestroyed01")
  end
  if uBBExtract02 then
    Event.Delete("uBBDestroyed02")
  end
  if uBBExtract03 then
    Event.Delete("uBBDestroyed03")
  end
  oExtractBB:CompletePart()
  if nBBExtracted < 3 then
    oExtractBB:RemoveTarget(Pg.GetGuidByName("JetCon001_BunkerBuster0" .. nBBindex))
  else
    Debug.Printf("********************* JET CON 001: SETTING CHECKPOINT 2 !!!")
    self:CopterAttackRegionActivate()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Jet01-41"
    })
    self:_SetFlag("JC001CP02")
    _Checkpoint({"CP02_P1", "CP02_P2"})
  end
end

function CancelExtractBB(self)
  self:_SetCancelMessage("[JetCon001.Terms.Cancel01]")
  oExtractBB.Cancel(oExtractBB)
end

function DestroyBunker(self)
  Debug.Printf("********************* JET CON 001: STARTING DESTROY BUNKER OBJECTIVE !!!")
  self:NearBunkerRegionActivate()
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Jet01-03",
    2,
    "Fiona-In-Mission-Contract-Jet01-44"
  })
  oDestroyBunker = self:CreateChild({
    sName = "Destroy the VZ Bunker",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "JetCon001_Bunker"
    },
    sDspShortDesc = "[JetCon001.Objectives.002]",
    tOnComplete = {
      {
        CompleteDestroyBunker,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
  nCallbackID = MrxPmc.SetStockpileChangeCallback("bunkerbuster", "==", 0, CancelDestroyBunker, {self})
end

function Cleanup(self)
  MrxPmc.DeleteStockpileChangeCallback(nCallbackID)
  MrxSupportData.SetJetPilotRecruited(false)
  local tLayersToRemove = {
    "Vz_State_JetCon001_CP01"
  }
  MrxLayerManager.Remove(tLayersToRemove)
  MrxTaskContract.Cleanup(self)
end

function CancelDestroyBunker(self)
  Debug.Printf("********************* JET CON 001: Bunker Buster callback just triggered, must have zero BBs!!!")
  self:_CreateEvent(Event.TimerRelative, {12}, function()
    if Pg.GetGuidByName("JetCon001_Bunker") then
      if Object.IsAlive(Pg.GetGuidByName("JetCon001_Bunker")) then
        Debug.Printf("********************* JET CON 001: BUNKER IS ALIVE AND YOU WASTED YOUR LAST BB !!!")
        self:_SetCancelMessage("[JetCon001.Terms.Cancel02]")
        oDestroyBunker.Cancel(oDestroyBunker)
      else
        Debug.Printf("********************* JET CON 001: BUNKER IS ALIVE AND YOU WASTED YOUR LAST BB !!!")
        oDestroyBunker.Complete(oDestroyBunker)
      end
    else
      Debug.Printf("********************* JET CON 001: NO BUNKER GUID - BAD JUJU!")
    end
  end)
end

function CompleteDestroyBunker(self)
  self:_CreateEvent(Event.TimerRelative, {5}, function()
    self.FionaCompleteVO(self)
  end)
end

function FionaCompleteVO(self)
  MrxVoSequence.Start({
    "Misha-In-Mission-Contract-Jet01-05",
    0.5,
    "Fiona-In-Mission-Contract-Jet01-06",
    1,
    {
      self.Complete,
      {self}
    }
  })
end

function FionaFailVO(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Jet01-06",
    1,
    {
      self.Cancel,
      {self}
    }
  })
end

function TravelMusicOnRegionActivate(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_JetCon001_TravelMusic"),
    "enter",
    false
  }, StartTravelMusic, {self})
end

function StartTravelMusic(self)
  Debug.Printf("********************* JET CON 001: START TRAVEL MUSIC")
  Sound.SetActionLevelsMusic(10, 0, 0, 0)
  Sound.LockActionLevelMusic(true)
  self:TravelMusicOffRegionActivate()
end

function TravelMusicOffRegionActivate(self)
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Region_JetCon001_Checkpoint"),
    "enter",
    false
  }, StopTravelMusic, {self})
end

function StopTravelMusic(self)
  Debug.Printf("********************* JET CON 001: STOP TRAVEL MUSIC")
  Sound.LockActionLevelMusic(false)
end
