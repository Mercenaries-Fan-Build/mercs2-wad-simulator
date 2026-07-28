function SetActiveContract(oContract)
  Debug.Printf("****************HIJACKCONTRACTMANAGER.SetActiveContract()")
  
  _oContract = oContract
end

function CompleteActiveContract()
  Debug.Printf("****************HIJACKCONTRACTMANAGER.CompleteActiveContract()")
  _oContract:Complete()
end

function CancelActiveContract()
  Debug.Printf("****************HIJACKCONTRACTMANAGER.CANCELACTIVECONTRACT()")
  _oContract:Cancel()
end
