inherit("MrxTaskContract")
import("MrxSubtitle")
import("DangerousBuilding")
import("MrxChiCon001Rescue")
import("MrxVoSequence")
import("MrxSupportData")
import("MrxSupportData")
import("MrxFactionManager")
import("MrxMusic")
tDB_Targets = {
  "_cumana_bld_hotelfourstar01 0x000b87ed",
  "_cumana_bld_hotelfourstar01 0x000b87ee"
}
tBldTargets = {
  "_cumana_bld_corner32x32B 0x001385a7",
  "_city_bld_apartment02 0x000b894b",
  "_city_bld_apartment01 0x000b894c"
}

function LoadAssets(self, tSaveData)
  local tLayersToAdd = {
    "vz_state_chicon001",
    "Vz_state_ChiCon001_Pristine"
  }
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  MrxLayerManager.Remove({
    "vz_state_cumana_act1ALL_N"
  })
  MrxLayerManager.Remove({
    "vz_state_cumana_act1all_staging"
  })
  BldgDeathCount = 0
  for i, building in pairs(tBldTargets) do
    local thisBldg = Pg.GetGuidByName(building)
    self:_CreateEvent(Event.ObjectDeath, {thisBldg}, function()
      BldgDeathCount = BldgDeathCount + 1
      BonusCompleteVO(self)
    end)
  end
  FionaVo(self)
  uTarget = Pg.GetGuidByName("PartyOfficial")
  oActionObjectve = self:CreateChild({
    sName = "Rescue_VIP",
    sModuleName = "MrxTaskObjectiveRelease",
    sActionLabel = "[ContextAction.RescuePrisoner]",
    vTgtInclude = uTarget,
    sDspShortDesc = "[ChiCon001.Objectives.001]",
    tOnPartComplete = {
      {
        ReleaseTarget,
        {self, uTarget}
      }
    },
    fOnCancel = function()
      Debug.Printf("Deliver objective failed")
      PrisonerUndelivered(self)
    end
  })
  local prisonerDeath = self:_CreateEvent(Event.ObjectDeath, {uTarget}, function()
    Debug.Printf("Prisoner died!")
    PrisonerUnrescued(self)
  end)
  self:_CreateEvent(Event.ObjectHibernation, {uTarget, "awake"}, SubdueTarget, {self, uTarget})
  self:_CreateEvent(Event.Boundary, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("LineRegion_Firefights"),
    "enter",
    false
  }, SetUpFirefights({self}))
  local nHealth = Object.GetHealth(uTarget)
  eChiVipDamage = Event.Create(Event.ObjectHealthLessThan, {
    uTarget,
    nHealth / 2
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi01-19",
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Chi01-20",
        jennifer = "Jennifer-In-Mission-Contract-Chi01-21",
        chris = "Chris-In-Mission-Contract-Chi01-22"
      }
    })
  end)
  MrxSupportData.AddFreebie("ChiCon001_RocketArtillery")
  DestroyAlliedBldgSetup(self)
end

function FionaVo(self)
  self:_CreateEvent(Event.TimerRelative, {3}, function()
    MrxVoSequence.Start({
      "Fiona-Banter-Contract-Chi01-01",
      {
        mattias = "Mattias-Banter-Contract-Chi01-02",
        jennifer = "Jennifer-Banter-Contract-Chi01-03",
        chris = "Chris-Banter-Contract-Chi01-04"
      }
    })
    self:_CreateEvent(Event.TimerRelative, {10}, function()
      MrxMusic.PlaySpecialMusic("mu_fac_ch_threat_01")
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Chi01-07",
        1,
        "Fiona-In-Mission-Contract-Chi01-08",
        1,
        "Fiona-In-Mission-Contract-Chi01-09",
        1,
        "Fiona-In-Mission-Contract-Chi01-10"
      })
    end)
  end)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("_cumana_bld_corner32x32B 0x001385a7"),
    "<",
    100,
    false,
    false
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi01-03"
    })
  end)
end

function SetUpFirefights(self)
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("ChineseSkirmish2")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("path_firefight2"),
    Priority = "lowPri",
    Mode = "Oneway",
    Start = "Nearest",
    Haste = 1
  })
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("ChineseSkirmish3")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("path_firefight3"),
    Priority = "lowPri",
    Mode = "Oneway",
    Start = "Nearest",
    Haste = 1
  })
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName("ChineseSkirmish4")),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("path_firefight4"),
    Priority = "lowPri",
    Mode = "Oneway",
    Start = "Nearest",
    Haste = 1
  })
end

function SubdueTarget(self, uTarget)
  Debug.Printf("ChiCon001: Subduing " .. tostring(Object.GetLocalizedName(uTarget)))
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi01-02"
  })
  MrxUtil.DisplayHealthBar(self, uTarget, 0, true, 0)
end

function ReleaseTarget(self, uTarget, tObjects)
  MrxMusic.PlaySpecialMusic("mu_fac_ch_kickass_01")
  Object.AddLabel(uTarget, "Prisoner")
  self:CreateChild({
    sName = "Deliver VIP",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = uTarget,
    uStartAttachedToPlayer = Player.GetLocalCharacter(),
    sDspShortDesc = "[ChiCon001.Objectives.002]",
    vDestLoc = "ChiCon001 Dropoff",
    bHumansFollow = true,
    fDist = 6,
    fOnComplete = function()
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Chi01-06",
        3,
        {
          self.Complete,
          {self}
        }
      })
    end,
    tOnCancel = {
      {
        PrisonerUndelivered,
        {self}
      }
    }
  })
end

function DisplayLifeBar(self, uGuid, nOldHealth)
  local nMaxHealth = Object.GetMaxHealth(uGuid)
  local nHealth = Object.GetHealth(uGuid)
  local nPercent = math.floor(nHealth / nMaxHealth * 100)
  local sColor = "[green]"
  MrxUtil.DisplayHealthBar(self, uTarget, 0, true, 0)
end

function DestroyAlliedBldgSetup(self)
  self:CreateChild({
    sName = "Bonus Objective: Destroy the Allied Buildings",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tBldTargets,
    sDspShortDesc = "[ChiCon001.Objectives.003]",
    bOptional = true,
    tOnComplete = {
      {
        BonusComplete,
        {self}
      }
    }
  })
end

function BonusCompleteVO(self)
  if BldgDeathCount == 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi01-12"
    })
  elseif BldgDeathCount == 2 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi01-13"
    })
  elseif BldgDeathCount == 3 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi01-14"
    })
  end
end

function BonusComplete(self)
  Debug.Printf("*******************ChiCon001: BONUS COMPLETE")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi01-15"
  })
  local isMultiplayer = Player.GetCurrentPlayers()
  if isMultiplayer == 2 then
    self:_SetPlayer1Bonus(50000)
    self:_SetPlayer2Bonus(50000)
  else
    self:_SetPlayer1Bonus(50000)
  end
end

function SpawnPatrols(self, sVeh, sPoint, sPath)
  Debug.Printf("*******************ChiCon001: SpawnPatrols")
  local uPoint = Pg.GetGuidByName(sPoint)
  local x, y, z = Object.GetPosition(uPoint)
  local uVeh = Pg.Spawn(sVeh, x, y, z, Object.GetYaw(uPoint), false, true)
  local h = Event.Create(Event.ObjectHibernation, {uVeh, "awake"}, ReinforcementPatrol, {
    self,
    uVeh,
    sPath
  })
end

function ReinforcementPatrol(self, sVeh, sPath)
  Debug.Printf("*******************ChiCon001: ReinforcementPatrol")
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(Pg.GetGuidByName(sVeh)),
    Goal = "PathMove",
    Target = Pg.GetGuidByName(sPath),
    Priority = "lowPri",
    Mode = "Oneway",
    Start = "Nearest",
    Haste = 0.1
  })
end

function PrisonerUnrescued(self)
  self:_SetCancelMessage("[ChiCon001.Terms.Cancel01]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi01-17",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function PrisonerUndelivered(self)
  self:_SetCancelMessage("[ChiCon001.Terms.Cancel02]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi01-18",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function Cleanup(self)
  Hud.ObjectiveTray:SetSlotToText({nSlot = 1, sText = " "})
  Hud.ObjectiveTray:SetSlotToText({nSlot = 2, sText = " "})
  MrxSupportData.RemoveFreebie("ChiCon001_RocketArtillery")
  MrxLayerManager.MarkForRemoval("vz_state_chicon001", "Vz_state_ChiCon001_Pristine")
  MrxMusic.StopSpecialMusic()
  MrxUtil.StopHealthBar(uTarget)
  MrxTaskContract.Cleanup(self)
end
