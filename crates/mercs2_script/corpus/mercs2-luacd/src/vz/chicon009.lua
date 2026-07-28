inherit("MrxTaskContract")
import("MrxTaskObjectiveDeliver")
import("MrxLayerManager")
import("MrxTimer")
import("MrxSubtitle")
import("MrxAchievements")
import("MrxMusic")
tDestroyLocs = {
  "Target1",
  "Target2",
  "Target3",
  "Target4",
  "Target5",
  "Target6",
  "Target7",
  "Target8"
}

function Activated(self)
  MrxTaskContract.Activated(self)
  MrxMusic.PlaySpecialMusic("mu_mission_chicon009_01")
  MrxLayerManager.Remove({
    "vz_state_cumana_act1ALL_S"
  })
  local uVehicle = Pg.GetGuidByName("ChiCon009_ZBD2000")
  Vehicle.Usable(uVehicle, false)
  local nTimeLimit = 600
  local nTimeToAdd = 30
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 1 then
    nTimeLimit = 500
    nTimeToAdd = 20
  elseif 2 <= nCompletions then
    nTimeLimit = 400
    nTimeToAdd = 15
  end
  oTimer = MrxTimer:Create({
    nStartTime = nTimeLimit,
    nWarning = nTimeLimit / 2,
    iTray = 2,
    tDoneCallbacks = {
      {
        OutOfTime,
        {self}
      }
    }
  })
  oTimer:Start()
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi09-01",
    3,
    "Fiona-In-Mission-Contract-Chi09-05",
    3,
    "Fiona-In-Mission-Contract-Chi09-18",
    3,
    "Fiona-In-Mission-Contract-Chi09-19"
  })
  local uVehicle = Pg.GetGuidByName("ChiCon009_ZBD2000")
  Vehicle.Usable(uVehicle, false)
  local oAcquireTankObjective = self:CreateChild({
    sName = "Acquire the ZBD2000",
    sModuleName = "MrxTaskObjectiveEnterVehicle",
    vTgtInclude = "ChiCon009_ZBD2000",
    nQuota = 1,
    uPlayer = Player.GetAnyCharacter(),
    sDspShortDesc = "[ChiCon009.Objectives.004]",
    tOnComplete = {
      {
        RendezvousAmbulance,
        {self}
      }
    },
    tOnCancel = {
      {
        self.VehicleUnentered,
        {self}
      }
    }
  })
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("ChiCon009_ZBD2000"),
    "D",
    "E"
  }, oAcquireTankObjective.Complete, {oAcquireTankObjective})
  FionaChiCon009Vo(self)
  AmbulanceSetup(self)
end

function RendezvousAmbulance(self)
  self:CreateChild({
    sName = "RendezvousAmbulance",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    vDestLoc = Pg.GetGuidByName("loc_AmbulanceRendezvous"),
    sDspShortDesc = "[ChiCon009.Objectives.005]",
    tOnComplete = {
      {
        AmbulanceObjective,
        {self}
      }
    },
    tOnCancel = {
      {
        self.MineActive,
        {self}
      }
    }
  })
end

function AmbulanceTemp(self)
  self:_CreateEvent(Event.TimerRelative, {10}, AmbulanceSetup, {self})
end

function AmbulanceSetup(self)
  Debug.Printf("*************ChiCon009: AmbulanceSetup")
  local spawnAmbulance = Pg.GetGuidByName("loc_SpawnAmbulance")
  local x, y, z = Object.GetPosition(spawnAmbulance)
  civAmbulance = Pg.Spawn("Ambulance (Driver)", x, y, z)
end

function AmbulanceObjective(self)
  MrxUtil.DisplayHealthBar(self, civAmbulance, 0, true, 0)
  local oAmbulanceObjective = self:CreateChild({
    sName = "Escort the Ambulance",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = civAmbulance,
    vDestLoc = Pg.GetGuidByName("loc_AmbulanceDropoff"),
    sDspBlpRdrIcon = "objective_defend",
    sDspShortDesc = "[ChiCon009.Objectives.003]",
    fOnComplete = function()
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Pmc01-10",
        3,
        {
          self.Complete,
          {self}
        }
      })
    end,
    tOnCancel = {
      {
        self.AmbulanceDestroyed,
        {self}
      }
    }
  })
  self:CreateChild({
    sName = "Special Case: Ambulance Destination Blip",
    sModuleName = "MrxTaskObjective",
    sDspShortDesc = "[ChiCon009.Objectives.006]",
    bTrackOnActivate = true,
    vTgtInclude = Pg.GetGuidByName("loc_AmbulanceDropoff"),
    fOnComplete = function()
    end,
    fOnCancel = function()
      self:OutOfTime()
    end
  })
  self:_CreateEvent(Event.ObjectProximity, {
    civAmbulance,
    Pg.GetGuidByName("loc_AmbulanceDropoff"),
    "<",
    10,
    false,
    false
  }, self.Complete, {self})
  self.eDeath = self:_CreateEvent(Event.ObjectDeath, {civAmbulance}, oAmbulanceObjective.Cancel, {oAmbulanceObjective})
  local nCompletions = self:GetNumCompletions()
  local uHaste = 0.1
  if nCompletions == 1 then
    uHaste = 0.2
  elseif 2 <= nCompletions then
    uHaste = 0.25
  end
  Ai.Goal({
    AIGuid = Vehicle.GetDriver(civAmbulance),
    Goal = "PathMove",
    Target = Pg.GetGuidByName("Path_Ambulance"),
    Priority = "lowPri",
    Timeout = 0,
    Mode = "Oneway",
    Start = "Nearest",
    Haste = 0.1,
    CallbackData = {
      {
        oAmbulanceObjective.Complete,
        {oAmbulanceObjective}
      }
    }
  })
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi09-08",
    3,
    {
      self.PlayAmbulanceMusic,
      {self}
    }
  })
  local nCivHib = Object.GetHibernationDistance(civAmbulance)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    civAmbulance,
    ">",
    nCivHib - 25,
    false,
    false
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi09-20"
    })
  end)
  self:_CreateEvent(Event.ObjectHibernation, {civAmbulance, "hibernated"}, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi09-20"
    })
    AmbulanceAbandoned(self)
  end)
end

function PlayAmbulanceMusic(self)
  MrxMusic.PlaySpecialMusic("mu_fac_oc_kickass_01")
end

function FionaChiCon009Vo(self)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Target4"),
    "<",
    60,
    false,
    false
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi09-14",
      5,
      {
        mattias = "Mattias-In-Mission-Contract-Chi09-15",
        jennifer = "Jennifer-In-Mission-Contract-Chi09-16",
        chris = "Chris-In-Mission-Contract-Chi09-17"
      }
    })
  end)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("Target1"),
    "<",
    75,
    false,
    false
  }, function()
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi09-02"
    })
  end)
end

function VehicleDeath(self)
  self:_SetCancelMessage("[ChiCon009.Terms.Cancel05]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi09-09",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function VehicleUnentered(self)
  self:_SetCancelMessage("[ChiCon009.Terms.Cancel01]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi09-10",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function MineActive(self)
  self:_SetCancelMessage("[ChiCon009.Terms.Cancel02]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi09-12",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function OutOfTime(self)
  self:_SetCancelMessage("[ChiCon009.Terms.Cancel04]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All04-14",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function AmbulanceDestroyed(self)
  self:_SetCancelMessage("[ChiCon009.Terms.Cancel03]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All04-14",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function AmbulanceAbandoned(self)
  self:_SetCancelMessage("[ChiCon009.Terms.Cancel06]")
  self.Cancel(self)
end

function Cleanup(self)
  MrxLayerManager.Add({
    "vz_state_cumana_act1ALL_S"
  })
  if oTimer then
    oTimer:Stop()
  end
  if Object.IsValid(civAmbulance) then
    MrxUtil.StopHealthBar(civAmbulance)
    Object.Remove(civAmbulance)
  end
  MrxMusic.StopSpecialMusic()
  MrxTaskContract.Cleanup(self)
end
