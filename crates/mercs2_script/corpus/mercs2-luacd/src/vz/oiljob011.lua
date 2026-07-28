inherit("MrxTaskJobVerifySet")
_tTargetNearbyVo = {
  {
    vSequence = "Fiona-In-Mission-Job-Oil11-01"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil11-04"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil11-01"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil11-04"
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil11-04"
  }
}
_tTargetCompleteVo = {
  {
    vSequence = "Fiona-In-Mission-Job-Oil11-14",
    tRange = {
      "[",
      1,
      9,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil11-15",
    tRange = {
      "[",
      1,
      9,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-All03-08",
    tRange = {
      "[",
      1,
      9,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-All03-09",
    tRange = {
      "[",
      1,
      9,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil11-16",
    tRange = {
      "[",
      2,
      9,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Oil11-17",
    tRange = {10}
  }
}

function LoadAssets(self, tSaveData)
  self:_AddTarget({
    sTarget = "OilJob011_Target_01",
    sDefenseLayer = "vz_State_OilJob011_01",
    sStagingLayer = "vz_State_OilJob011_01_staging",
    sPristineLayer = "vz_State_OilJob011_01_pristine"
  })
  self:_AddTarget({
    sTarget = "OilJob011_Target_02",
    sDefenseLayer = "vz_State_OilJob011_02",
    sStagingLayer = "vz_State_OilJob011_02_staging",
    sPristineLayer = "vz_State_OilJob011_02_pristine"
  })
  self:_AddTarget({
    sTarget = "OilJob011_Target_03",
    sDefenseLayer = "vz_State_OilJob011_03",
    sStagingLayer = "vz_State_OilJob011_03_staging",
    sPristineLayer = "vz_State_OilJob011_03_pristine"
  })
  self:_AddTarget({
    sTarget = "OilJob011_Target_04",
    sDefenseLayer = "vz_State_OilJob011_04",
    sStagingLayer = "vz_State_OilJob011_04_staging",
    sPristineLayer = "vz_State_OilJob011_04_pristine"
  })
  self:_AddTarget({
    sTarget = "OilJob011_Target_05",
    sDefenseLayer = "vz_State_OilJob011_05",
    sStagingLayer = "vz_State_OilJob011_05_staging",
    sPristineLayer = "vz_State_OilJob011_05_pristine"
  })
  for i = 1, 5 do
    local n = string.format("%02d", i)
    self:_AddTarget({
      sTarget = "OilJob012_Target_" .. n,
      sDefenseLayer = "vz_State_OilJob012_" .. n,
      sStagingLayer = "vz_State_OilJob011_" .. n .. "_staging",
      sPristineLayer = "vz_State_OilJob011_" .. n .. "_pristine"
    })
  end
  MrxTaskJobVerifySet.LoadAssets(self, tSaveData)
end

function Activated(self)
  MrxTaskJobVerifySet.Activated(self)
  self:_SetFactionId("Oil")
  self:_SetTargetNearbyVo(_tTargetNearbyVo)
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  self:_Go()
end
