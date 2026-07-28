Debug.Printf("Adding asset model")

function Init()
  Debug.Printf("Init called")
  SetStaging()
end

function SetStaging(self)
  Debug.Printf("SetStaging Called")
  local count = 0
  local stageTable = Pg.GetGuidByName("OilLif001 Table")
  local tRiders = Vehicle.GetRiders(stageTable)
  for i, rider in pairs(tRiders) do
    if count == 0 then
      iRider1 = rider
      count = count + 1
    else
      iRider2 = rider
    end
  end
  Human.SetState(iRider1, "InVehicle", "lifestylejobPlayerArmwrestlingWinningloop01")
  Human.SetState(iRider2, "InVehicle", "lifestylejobOpponentArmwrestlingWinningloop01")
  Debug.Printf("At end of function")
end
